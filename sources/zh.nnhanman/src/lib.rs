#![no_std]

use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, Home, HomeComponent,
	HomeLayout, Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, MangaWithChapter,
	Page, PageContent, Result, Source, UpdateStrategy, Viewer,
	alloc::{String, Vec, format, string::ToString, vec},
	imports::{
		html::{Document, Element},
		net::Request,
		std::send_partial_result,
	},
	prelude::*,
};

const BASE_URL: &str = "https://nnhanman.xyz";

struct NnHanman;

impl Source for NnHanman {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let url = match query {
			Some(query) if !query.trim().is_empty() => {
				format!("{BASE_URL}/catalog.php?key={}", encode_query(query.trim()))
			}
			_ => listing_url("latest", page),
		};
		parse_manga_page(&url)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let manga_url = absolute_url(&manga.key);
		let html = Request::get(&manga_url)?.html()?;

		if needs_details {
			manga.title = html
				.select_first(".Introduct_Sub h1")
				.and_then(|el| el.text())
				.map(clean_title)
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first("#Cover img")
				.and_then(|img| attr_url(&img, "data-original").or_else(|| attr_url(&img, "src")));
			manga.authors = html
				.select(".sub_r .txtItme")
				.and_then(|els| {
					els.skip(0)
						.find_map(|el| {
							let text = clean_text(el.text()?);
							if text.contains("连载") || text.contains("完结") || text.contains(',') {
								None
							} else {
								Some(vec![text])
							}
						})
				});
			manga.tags = html.select(".sub_r .txtItme a").map(|els| {
				els.filter_map(|el| el.text().map(clean_text))
					.filter(|text| !text.is_empty())
					.collect()
			});
			manga.description = html
				.select_first(".txtDesc")
				.and_then(|el| el.text())
				.map(|text| clean_text(text).replace("介绍:", ""));
			manga.status = html
				.select_first(".sub_r .date")
				.and_then(|el| el.text())
				.map(|text| {
					if text.contains("完结") {
						MangaStatus::Completed
					} else if text.contains("连载") {
						MangaStatus::Ongoing
					} else {
						MangaStatus::Unknown
					}
				})
				.unwrap_or(MangaStatus::Unknown);
			manga.content_rating = ContentRating::NSFW;
			manga.update_strategy = match manga.status {
				MangaStatus::Completed | MangaStatus::Cancelled => UpdateStrategy::Never,
				_ => UpdateStrategy::Always,
			};
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(manga_url.clone());

			if needs_chapters {
				send_partial_result(&manga);
			}
		}

		if needs_chapters {
			manga.chapters = html.select("#mh-chapter-list-ol-0 a").map(|els| {
				els.filter_map(|a| {
					let href = a.attr("href")?;
					let title = a.text().map(clean_text);
					Some(Chapter {
						key: normalize_key(&href),
						title: title.clone(),
						chapter_number: title.as_ref().and_then(|value| parse_chapter_number(value)),
						url: Some(absolute_url(&href)),
						..Default::default()
					})
				})
				.collect()
			});
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let html = Request::get(absolute_url(&chapter.key))?.html()?;
		Ok(html
			.select(".view-imgBox img")
			.map(|imgs| {
				imgs.filter_map(|img| {
					let url = attr_url(&img, "data-original").or_else(|| attr_url(&img, "src"))?;
					Some(Page {
						content: PageContent::Url(url, None),
						..Default::default()
					})
				})
				.collect()
			})
			.unwrap_or_default())
	}
}

impl ListingProvider for NnHanman {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl Home for NnHanman {
	fn get_home(&self) -> Result<HomeLayout> {
		let html = Request::get(BASE_URL)?.html()?;
		let latest = parse_manga_with_chapters(&html, "最近更新");
		let new_books = parse_section_manga(&html, "新书发布");
		let recommended = parse_section_manga(&html, "推荐漫画");
		let completed = parse_section_manga(&html, "已完结");

		Ok(HomeLayout {
			components: vec![
				HomeComponent {
					title: Some("最近更新".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::MangaChapterList {
						page_size: Some(9),
						entries: latest,
						listing: None,
					},
				},
				HomeComponent {
					title: Some("新书发布".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::Scroller {
						entries: new_books.into_iter().map(|m| m.into()).collect(),
						listing: None,
					},
				},
				HomeComponent {
					title: Some("推荐漫画".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::Scroller {
						entries: recommended.into_iter().map(|m| m.into()).collect(),
						listing: None,
					},
				},
				HomeComponent {
					title: Some("已完结".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::MangaList {
						ranking: false,
						page_size: Some(9),
						entries: completed.into_iter().map(|m| m.into()).collect(),
						listing: None,
					},
				},
			],
		})
	}
}

impl DeepLinkHandler for NnHanman {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = normalize_key(&url);
		if key.contains("/chapter-") {
			let manga_key = key
				.split("/chapter-")
				.next()
				.map(|part| format!("{part}.html"))
				.unwrap_or_default();
			Ok(Some(DeepLinkResult::Chapter { manga_key, key }))
		} else if key.starts_with("/comic/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.html()?;
	let entries = html
		.select("ul.col_3_1 > li")
		.map(|els| els.filter_map(|el| parse_card(&el)).collect())
		.unwrap_or_default();
	let has_next_page = html
		.select(".pages a, .page a")
		.map(|els| els.filter_map(|el| el.text()).any(|text| text.contains("下一")))
		.unwrap_or(false);
	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

fn parse_card(el: &Element) -> Option<Manga> {
	let link = el.select_first("a.ImgA").or_else(|| el.select_first("a.txtA"))?;
	let href = link.attr("href")?;
	let title = link
		.attr("title")
		.or_else(|| el.select_first("a.txtA").and_then(|a| a.text()))
		.map(clean_text)?;
	if !href.starts_with("/comic/") || href.contains("/chapter-") || title.is_empty() {
		return None;
	}
	Some(Manga {
		key: normalize_key(&href),
		title,
		cover: el
			.select_first("img")
			.and_then(|img| attr_url(&img, "data-original").or_else(|| attr_url(&img, "src"))),
		url: Some(absolute_url(&href)),
		content_rating: ContentRating::NSFW,
		viewer: Viewer::Webtoon,
		..Default::default()
	})
}

fn parse_section_manga(html: &Document, title: &str) -> Vec<Manga> {
	html.select(".imgBox")
		.map(|sections| {
			sections
				.filter(|section| {
					section
						.select_first(".Sub_H2 .Title")
						.and_then(|el| el.text())
						.map_or(false, |text| clean_text(text) == title)
				})
				.next()
				.and_then(|section| {
					section
						.select("ul.col_3_1 > li")
						.map(|els| els.filter_map(|el| parse_card(&el)).collect())
				})
				.unwrap_or_default()
		})
		.unwrap_or_default()
}

fn parse_manga_with_chapters(
	html: &Document,
	section_title: &str,
) -> Vec<MangaWithChapter> {
	html.select(".imgBox")
		.map(|sections| {
			sections
				.filter(|section| {
					section
						.select_first(".Sub_H2 .Title")
						.and_then(|el| el.text())
						.map_or(false, |text| clean_text(text) == section_title)
				})
				.next()
				.and_then(|section| {
					section.select("ul.col_3_1 > li").map(|els| {
						els.filter_map(|el| {
							let manga = parse_card(&el)?;
							let chapter_link = el.select_first(".info a")?;
							let href = chapter_link.attr("href")?;
							let title = chapter_link.text().map(clean_text);
							Some(MangaWithChapter {
								manga,
								chapter: Chapter {
									key: normalize_key(&href),
									title: title.clone(),
									chapter_number: title
										.as_ref()
										.and_then(|value| parse_chapter_number(value)),
									url: Some(absolute_url(&href)),
									..Default::default()
								},
							})
						})
						.collect()
					})
				})
				.unwrap_or_default()
		})
		.unwrap_or_default()
}

fn listing_url(id: &str, page: i32) -> String {
	let path = match id {
		"newbook" => "/update/newbook",
		"completed" => "/comics/all/ob/time/st/completed",
		_ => "/update",
	};
	if page <= 1 {
		format!("{BASE_URL}{path}")
	} else {
		format!("{BASE_URL}{path}/{page}")
	}
}

fn normalize_key(url: &str) -> String {
	url.strip_prefix(BASE_URL).unwrap_or(url).to_string()
}

fn absolute_url(url: &str) -> String {
	if url.starts_with("http") {
		url.into()
	} else {
		format!("{BASE_URL}{url}")
	}
}

fn attr_url(el: &Element, name: &str) -> Option<String> {
	el.attr(name).map(|url| {
		if url.starts_with("//") {
			format!("https:{url}")
		} else if url.starts_with('/') {
			format!("{BASE_URL}{url}")
		} else {
			url
		}
	})
}

fn clean_text(text: String) -> String {
	text.replace('\n', " ")
		.replace('\t', " ")
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
}

fn clean_title(text: String) -> String {
	clean_text(text).replace('《', "").replace('》', "")
}

fn parse_chapter_number(title: &str) -> Option<f32> {
	let start = title.find('第')?;
	let rest = &title[start + '第'.len_utf8()..];
	let digits = rest
		.chars()
		.take_while(|ch| ch.is_ascii_digit() || *ch == '.')
		.collect::<String>();
	digits.parse().ok()
}

fn encode_query(input: &str) -> String {
	let mut output = String::new();
	for byte in input.as_bytes() {
		if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
			output.push(*byte as char);
		} else if *byte == b' ' {
			output.push_str("%20");
		} else {
			let high = byte >> 4;
			let low = byte & 0x0f;
			output.push('%');
			output.push(hex(high));
			output.push(hex(low));
		}
	}
	output
}

fn hex(value: u8) -> char {
	match value {
		0..=9 => (b'0' + value) as char,
		_ => (b'A' + value - 10) as char,
	}
}

register_source!(NnHanman, ListingProvider, Home, DeepLinkHandler);
