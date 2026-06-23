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

const BASE_URL: &str = "https://kxmanhua.com";

struct KxManhua;

impl Source for KxManhua {
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
				format!("{BASE_URL}/manga/search?keyword={}&page={page}", encode_query(query.trim()))
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
				.select_first(".anime__details__title h3")
				.and_then(|el| el.text())
				.map(clean_text)
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first(".anime__details__pic")
				.and_then(|el| attr_url(&el, "data-setbg"));
			manga.authors = html
				.select_first(".anime__details__title span")
				.and_then(|el| el.text())
				.map(|text| vec![clean_text(text).replace("作者：", "")]);
			manga.description = html.select(".anime__details__text > p").and_then(|els| {
				els.filter_map(|el| el.text().map(clean_text))
					.find(|text| !text.is_empty())
			});
			manga.tags = html
				.select(".anime__details__widget li")
				.and_then(|els| {
					els.filter_map(|el| el.text())
						.find(|text| text.contains("标签："))
						.map(|text| {
							clean_text(text)
								.replace("标签：", "")
								.split(' ')
								.filter(|tag| !tag.is_empty())
								.map(String::from)
								.collect()
						})
				});
			manga.status = html
				.select_first(".anime__details__pic .ep, .anime__details__pic .epgreen")
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
			manga.chapters = html.select(".chapter_list a").map(|els| {
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
			.select(".blog__details__content > img")
			.map(|imgs| {
				imgs.filter_map(|img| {
					let url = attr_url(&img, "src")?;
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

impl ListingProvider for KxManhua {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		parse_manga_page(&listing_url(&listing.id, page))
	}
}

impl Home for KxManhua {
	fn get_home(&self) -> Result<HomeLayout> {
		let html = Request::get(BASE_URL)?.html()?;
		let latest = parse_home_chapter_section(&html, "最近更新");
		let newest = parse_home_manga_section(&html, "最新上架");
		let popular = parse_home_manga_section(&html, "本周热门");

		Ok(HomeLayout {
			components: vec![
				HomeComponent {
					title: Some("最近更新".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::MangaChapterList {
						page_size: Some(8),
						entries: latest,
						listing: None,
					},
				},
				HomeComponent {
					title: Some("最新上架".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::Scroller {
						entries: newest.into_iter().map(|m| m.into()).collect(),
						listing: None,
					},
				},
				HomeComponent {
					title: Some("本周热门".into()),
					subtitle: None,
					value: aidoku::HomeComponentValue::MangaList {
						ranking: true,
						page_size: Some(8),
						entries: popular.into_iter().map(|m| m.into()).collect(),
						listing: None,
					},
				},
			],
		})
	}
}

impl DeepLinkHandler for KxManhua {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if !url.starts_with(BASE_URL) {
			return Ok(None);
		}
		let key = normalize_key(&url);
		if key.contains("/detail/") {
			let manga_key = key
				.split("/detail/")
				.next()
				.map(String::from)
				.unwrap_or_default();
			Ok(Some(DeepLinkResult::Chapter { manga_key, key }))
		} else if key.starts_with("/manga/") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

fn parse_manga_page(url: &str) -> Result<MangaPageResult> {
	let html = Request::get(url)?.html()?;
	let entries = html
		.select(".product__item")
		.map(|els| els.filter_map(|el| parse_card(&el)).collect())
		.unwrap_or_default();
	let has_next_page = html
		.select(".product__pagination a")
		.map(|els| els.filter_map(|el| el.text()).any(|text| text.contains("下一页")))
		.unwrap_or(false);
	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

fn parse_card(el: &Element) -> Option<Manga> {
	let link = el.select_first(".product__item__text a")?;
	let href = link.attr("href")?;
	let title = link
		.attr("title")
		.or_else(|| link.text())
		.map(clean_title)?;
	if !href.starts_with("/manga/") || href.contains("/detail/") || title.is_empty() {
		return None;
	}
	let status = el
		.select_first(".ep, .epgreen")
		.and_then(|badge| badge.text())
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
	Some(Manga {
		key: normalize_key(&href),
		title,
		cover: el
			.select_first(".product__item__pic")
			.and_then(|pic| attr_url(&pic, "data-setbg")),
		status,
		content_rating: ContentRating::NSFW,
		viewer: Viewer::Webtoon,
		url: Some(absolute_url(&href)),
		..Default::default()
	})
}

fn parse_sidebar_card(el: &Element) -> Option<Manga> {
	let link = el.select_first("p a")?;
	let href = link.attr("href")?;
	let title = link
		.attr("title")
		.or_else(|| link.text())
		.map(clean_title)?;
	Some(Manga {
		key: normalize_key(&href),
		title,
		cover: attr_url(el, "data-setbg"),
		content_rating: ContentRating::NSFW,
		viewer: Viewer::Webtoon,
		url: Some(absolute_url(&href)),
		..Default::default()
	})
}

fn parse_home_manga_section(html: &Document, title: &str) -> Vec<Manga> {
	html.select(".trending__product, .product__sidebar__view")
		.map(|sections| {
			sections
				.filter(|section| {
					section
						.select_first(".section-title h4, .section-title h5")
						.and_then(|el| el.text())
						.map_or(false, |text| clean_text(text) == title)
				})
				.next()
				.and_then(|section| {
					section
						.select(".product__item")
						.map(|els| els.filter_map(|el| parse_card(&el)).collect::<Vec<_>>())
						.or_else(|| {
							section.select(".product__sidebar__view__item").map(|els| {
								els.filter_map(|el| parse_sidebar_card(&el)).collect::<Vec<_>>()
							})
						})
				})
				.unwrap_or_default()
		})
		.unwrap_or_default()
}

fn parse_home_chapter_section(
	html: &Document,
	title: &str,
) -> Vec<MangaWithChapter> {
	parse_home_manga_section(html, title)
		.into_iter()
		.map(|manga| MangaWithChapter {
			manga,
			chapter: Chapter::default(),
		})
		.collect()
}

fn listing_url(id: &str, page: i32) -> String {
	let orderby = match id {
		"popular" => 1,
		"newest" => 3,
		_ => 2,
	};
	format!("{BASE_URL}/manga/library?type=0&complete=1&page={page}&orderby={orderby}")
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
	clean_text(text).replace("漫画", "")
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

register_source!(KxManhua, ListingProvider, Home, DeepLinkHandler);
