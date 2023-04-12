mod configuration;

use clap::Parser;
use configuration::AppConfig;
use dialoguer::{theme::ColorfulTheme, Password};
use notion::{
    ids::{BlockId, DatabaseId, PageId},
    models::{
        block::{Block, FileObject},
        paging::Pageable,
        search::{NotionSearch, SearchRequest},
        text::RichText,
    },
    NotionApi,
};
use std::{collections::HashMap, str::FromStr};
use tokio::io::AsyncWriteExt;

#[derive(Parser)]
#[command()]
struct Cli {
    // #[arg(short, long)]
    // element_id: Option<String>,

    // #[arg(short, long)]
    // text: Option<String>,
    #[arg(short, long)]
    save_token: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let term_theme = ColorfulTheme::default();

    if cli.save_token {
        let api_key: String = Password::with_theme(&term_theme)
            .with_prompt("Notion api key:")
            .interact()?;
        let config = AppConfig::new(api_key);
        config.save_user_config()?;
        return Ok(());
    }

    let config = configuration::AppConfig::load_user_config()?;
    let notion_api = NotionApi::new(config.notion_api_key)?;

    // let search_query = NotionSearch::Filter {
    //     property: notion::models::search::FilterProperty::Object,
    //     value: notion::models::search::FilterValue::Page,
    // };

    let search_query = NotionSearch::Query(String::from(""));

    let mut search_result = notion_api.search(search_query).await?;

    loop {
        for object in search_result.results {
            match object {
                notion::models::Object::Block { block: _ } => {
                    println!("Block");
                }
                notion::models::Object::Database { database } => {
                    println!(
                        "Database: {} {}",
                        database.title_plain_text(),
                        notion_database_id_to_url(&database.id)
                    );
                }
                notion::models::Object::Page { page } => {
                    let title = page.title().unwrap();
                    println!("Page: {} {}", title, notion_page_id_to_url(&page.id));
                    if let Err(error) = process_page(&notion_api, page.id).await {
                        eprintln!("Failed for {title} with error {error:?}");
                    }
                }
                notion::models::Object::List { list: _ } => {
                    println!("List");
                }
                notion::models::Object::User { user: _ } => {
                    println!("User");
                }
                notion::models::Object::Error { error: _ } => {
                    println!("Error");
                }
            }
        }
        if let Some(cursor) = search_result.next_cursor {
            let search_request = SearchRequest::default().start_from(Some(cursor));
            search_result = notion_api.search(search_request).await?;
        } else {
            break;
        }
    }

    Ok(())
}

async fn process_page(notion_client: &NotionApi, page_id: PageId) -> anyhow::Result<()> {
    let page = notion_client.get_page(page_id.clone()).await?;
    let page_title = page.title().expect("failed to get page title");

    let block_id: BlockId = page_id.into();
    let mut children = notion_client.get_block_children(block_id.clone()).await?;

    let mut page_buffer = String::new();

    let _page_id_cache = PageIdCache::new();

    loop {
        for child in children.results {
            block_to_markdown(child, &mut page_buffer)?;
        }
        if let Some(cursor) = children.next_cursor {
            children = notion_client
                .get_block_children_with_cursor(block_id.clone(), cursor)
                .await?;
        } else {
            break;
        }
    }

    let page_title = page_title.replace('/', "-");
    let mut file = tokio::fs::File::create(format!("output/{page_title}.md")).await?;
    file.write_all(page_buffer.as_bytes()).await?;

    Ok(())
}

#[allow(clippy::print_with_newline)]
#[allow(clippy::write_with_newline)]
fn block_to_markdown(block: Block, writer_buffer: &mut dyn std::fmt::Write) -> anyhow::Result<()> {
    match block {
        Block::Paragraph {
            common: _,
            paragraph,
        } => {
            write!(
                writer_buffer,
                "{}\n",
                render_rich_text(&paragraph.rich_text)
            )?;
            for child in paragraph.children.unwrap_or_default() {
                block_to_markdown(child, writer_buffer)?;
            }
        }
        Block::Heading1 {
            common: _,
            heading_1,
        } => {
            write!(
                writer_buffer,
                "\n# {}\n\n",
                render_rich_text(&heading_1.rich_text)
            )?;
        }
        Block::Heading2 {
            common: _,
            heading_2,
        } => {
            write!(
                writer_buffer,
                "\n## {}\n\n",
                render_rich_text(&heading_2.rich_text)
            )?;
        }
        Block::Heading3 {
            common: _,
            heading_3,
        } => {
            write!(
                writer_buffer,
                "\n### {}\n\n",
                render_rich_text(&heading_3.rich_text)
            )?;
        }
        Block::Callout { common: _, callout } => {
            // TODO: Add support for callout icon
            write!(
                writer_buffer,
                "> [!info]\n{}\n",
                render_rich_text(&callout.rich_text)
                    .lines()
                    .map(|line| format!("> {}\n", line))
                    .collect::<String>()
            )?;
        }
        Block::Quote { common: _, quote } => {
            write!(writer_buffer, "> {}\n", render_rich_text(&quote.rich_text))?;
            write!(writer_buffer, "START QUOTE CHILDREN:\n")?;
            for child in quote.children.unwrap_or_default() {
                block_to_markdown(child, writer_buffer)?;
            }
            write!(writer_buffer, "END QUOTE CHILDREN:\n")?;
        }
        Block::BulletedListItem {
            common: _,
            bulleted_list_item,
        } => {
            write!(
                writer_buffer,
                "* {}\n",
                render_rich_text(&bulleted_list_item.rich_text)
            )?;
            write!(writer_buffer, "START BULLET CHILDREN:\n")?;
            for child in bulleted_list_item.children.unwrap_or_default() {
                block_to_markdown(child, writer_buffer)?;
            }
            write!(writer_buffer, "END BULLET CHILDREN:\n")?;
        }
        Block::NumberedListItem {
            common: _,
            numbered_list_item,
        } => {
            write!(
                writer_buffer,
                "1. {}\n",
                render_rich_text(&numbered_list_item.rich_text)
            )?;
            write!(writer_buffer, "START NUMBERED CHILDREN:\n")?;
            for child in numbered_list_item.children.unwrap_or_default() {
                block_to_markdown(child, writer_buffer)?;
            }
            write!(writer_buffer, "END NUMBERED CHILDREN:\n")?;
        }
        Block::Toggle { common: _, toggle } => {
            let summary = render_rich_text(&toggle.rich_text);

            write!(writer_buffer, "<details> <summary>{summary}</summary> \n",)?;

            for child in toggle.children.unwrap_or_default() {
                block_to_markdown(child, writer_buffer)?;
            }

            write!(writer_buffer, "</details>\n\n",)?;
        }
        Block::ToDo { common: _, to_do } => {
            let checked = to_do.checked;
            let checked = if checked { "x" } else { "" };
            write!(
                writer_buffer,
                "- [{checked}] {}\n",
                render_rich_text(&to_do.rich_text)
            )?;

            write!(writer_buffer, "START TODO CHILDREN:\n")?;
            for child in to_do.children.unwrap_or_default() {
                block_to_markdown(child, writer_buffer)?;
            }
            write!(writer_buffer, "END TODO CHILDREN:\n")?;
        }
        Block::Code { common: _, code } => {
            let content = render_rich_text(&code.rich_text);
            // this works
            let language = format!("{:?}", code.language).to_lowercase();
            // todo caption

            write!(writer_buffer, "\n```{language}\n{content}\n```\n\n",)?;
        }
        Block::ChildPage { common, child_page } => {
            // I think this is right?
            let block_id = common.id;
            let _page_id = PageId::from_str(&block_id.to_string())?;
            // wait is this needed?
            // let _page_title = page_id_cache.get_page_title(&page_id)?;

            write!(writer_buffer, "Child page: [[{}]]\n", child_page.title)?;
        }
        Block::ChildDatabase {
            common: _,
            child_database,
        } => {
            // TODO same as above?
            write!(writer_buffer, "Child database: {}\n", child_database.title)?;
        }
        Block::Image { common: _, image } => {
            write!(writer_buffer, "![[{}]]\n", render_file_object(image))?;
        }
        Block::Video { common: _, video } => {
            write!(writer_buffer, "![[{}]]\n", render_file_object(video))?;
        }
        Block::File {
            common: _,
            file,
            caption: _,
        } => {
            write!(writer_buffer, "![[{}]]\n", render_file_object(file))?;
        }
        Block::Pdf { common: _, pdf } => {
            write!(writer_buffer, "![[{}]]\n", render_file_object(pdf))?;
        }

        Block::Divider { common: _ } => {
            write!(writer_buffer, "----\n")?;
        }

        Block::Embed { common: _, embed } => {
            write!(writer_buffer, "![[{}]]\n", embed.url)?;
        }

        Block::Bookmark {
            common: _,
            bookmark,
        } => {
            let caption = render_rich_text(&bookmark.caption);
            write!(
                writer_buffer,
                "caption {} \n![[{}]]\n",
                caption, bookmark.url
            )?;
        }
        Block::Equation {
            common: _,
            equation,
        } => {
            write!(writer_buffer, "Equation {}\n", equation.expression)?;
        }

        Block::TableOfContents {
            common: _,
            table_of_contents: _,
        } => {
            write!(writer_buffer, "\nTABLE OF CONTENTS\n")?;
        }
        Block::Breadcrumb { common: _ } => {
            write!(writer_buffer, "\nBREADCRUMB\n")?;
        }
        Block::ColumnList {
            common: _,
            column_list,
        } => {
            for child in column_list.children {
                write!(writer_buffer, "COLUMN LIST\n\n")?;
                block_to_markdown(child, writer_buffer)?;
                write!(writer_buffer, "COLUMN LIST END\n\n")?;
            }
        }
        Block::Column { common: _, column } => {
            for child in column.children {
                write!(writer_buffer, "COLUMN LIST\n\n")?;
                block_to_markdown(child, writer_buffer)?;
                write!(writer_buffer, "COLUMN LIST END\n\n")?;
            }
        }
        Block::LinkPreview {
            common: _,
            link_preview,
        } => {
            write!(writer_buffer, "![[{}]]\n", link_preview.url)?;
        }
        Block::Template {
            common: _,
            template,
        } => {
            let content = render_rich_text(&template.rich_text);
            write!(writer_buffer, "\nTEMPLATE {}\n", content)?;
        }
        Block::LinkToPage {
            common: _,
            link_to_page: _,
        } => {
            write!(writer_buffer, "\nLINK TO PAGE\n")?;
        }
        Block::Table {
            common: _,
            table: _,
        } => {
            write!(writer_buffer, "\nTABLE\n")?;
        }
        Block::SyncedBlock {
            common: _,
            synced_block: _,
        } => {
            write!(writer_buffer, "\nSYNCED BLOCK\n")?;
        }
        Block::TableRow {
            common: _,
            table_row: _,
        } => {
            write!(writer_buffer, "\nTABLE ROW\n")?;
        }
        Block::Unsupported { common: _ } => {
            write!(writer_buffer, "\nUNSUPPORTED\n")?;
        }
        Block::Unknown => {
            write!(writer_buffer, "\nUNKNOWN\n")?;
        }
    }
    Ok(())
}

fn render_file_object(file_object: FileObject) -> String {
    match file_object {
        FileObject::File { file } => {
            // url is private?
            file.url
        }
        FileObject::External { external } => external.url,
    }
}

fn render_rich_text(rich_text: &[RichText]) -> String {
    rich_text
        .iter()
        .map(|text| text.plain_text())
        .collect::<String>()
}

fn notion_page_id_to_url(id: &PageId) -> String {
    let id_stripped = id.to_string().replace('-', "");
    format!("http://notion.so/{}", id_stripped)
}

fn notion_database_id_to_url(id: &DatabaseId) -> String {
    let id_stripped = id.to_string().replace('-', "");
    format!("http://notion.so/{}", id_stripped)
}

#[allow(dead_code)]
struct PageIdCache {
    page_to_title: HashMap<PageId, String>,
}

impl PageIdCache {
    fn new() -> Self {
        Self {
            page_to_title: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    async fn get_page_title(&mut self, id: &PageId, client: &NotionApi) -> anyhow::Result<String> {
        if let Some(title) = self.page_to_title.get(id) {
            Ok(title.clone())
        } else {
            let page = client.get_page(id.clone()).await?;
            let title = page.title().unwrap_or("UNKNOWN_TITLE".to_owned());
            self.page_to_title.insert(id.clone(), title.clone());
            Ok(title)
        }
    }
}

#[allow(dead_code)]
fn internal_embed(text: Option<&str>, link: &str) -> String {
    if let Some(text) = text {
        format!("![[{}|{}]]", link, text)
    } else {
        format!("![[{}]]", link)
    }
}

#[allow(dead_code)]
fn external_embed(text: Option<&str>, link: &str) -> String {
    // should I care about url encoding here?
    if let Some(text) = text {
        format!("![{}]({})", text, link)
    } else {
        format!("![]({})", link)
    }
}

#[allow(dead_code)]
fn internal_link(text: Option<&str>, link: &str) -> String {
    if let Some(text) = text {
        format!("[[{}|{}]]", link, text)
    } else {
        format!("[[{}]]", link)
    }
}

#[allow(dead_code)]
fn external_link(text: Option<&str>, link: &str) -> String {
    // should I care about url encoding here?
    if let Some(text) = text {
        format!("[{}]({})", text, link)
    } else {
        format!("<{}>", link)
    }
}
