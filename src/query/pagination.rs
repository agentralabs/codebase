use serde::{Deserialize, Serialize};

/// A cursor-based page of results.
///
/// Uses opaque string cursors for stable, token-efficient pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPage<T> {
    /// The items in this page.
    pub items: Vec<T>,
    /// The cursor pointing to the next page, if any.
    pub next_cursor: Option<String>,
    /// Whether there are more results after this page.
    pub has_more: bool,
    /// Total number of items across all pages (if known).
    pub total: Option<usize>,
}

impl<T: Clone> CursorPage<T> {
    /// Create a page from a slice using cursor-based pagination.
    ///
    /// The `cursor` is a 0-based offset encoded as a string.
    /// If `cursor` is `None`, starts from the beginning.
    /// `limit` controls how many items to include in this page.
    pub fn from_slice(data: &[T], cursor: Option<&str>, limit: usize) -> Self {
        let offset = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);

        let total_len = data.len();

        if offset >= total_len {
            return Self {
                items: Vec::new(),
                next_cursor: None,
                has_more: false,
                total: Some(total_len),
            };
        }

        let end = (offset + limit).min(total_len);
        let items = data[offset..end].to_vec();
        let has_more = end < total_len;
        let next_cursor = if has_more {
            Some(end.to_string())
        } else {
            None
        };

        Self {
            items,
            next_cursor,
            has_more,
            total: Some(total_len),
        }
    }

    /// Create an empty page.
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
            total: Some(0),
        }
    }

    /// Number of items in this page.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether this page is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Map the items in this page to a different type.
    pub fn map<U: Clone>(self, f: impl Fn(T) -> U) -> CursorPage<U> {
        CursorPage {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
            has_more: self.has_more,
            total: self.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_slice_first_page() {
        let data: Vec<i32> = (0..10).collect();
        let page = CursorPage::from_slice(&data, None, 3);
        assert_eq!(page.items, vec![0, 1, 2]);
        assert!(page.has_more);
        assert_eq!(page.next_cursor, Some("3".to_string()));
        assert_eq!(page.total, Some(10));
    }

    #[test]
    fn test_from_slice_middle_page() {
        let data: Vec<i32> = (0..10).collect();
        let page = CursorPage::from_slice(&data, Some("3"), 3);
        assert_eq!(page.items, vec![3, 4, 5]);
        assert!(page.has_more);
        assert_eq!(page.next_cursor, Some("6".to_string()));
    }

    #[test]
    fn test_from_slice_last_page() {
        let data: Vec<i32> = (0..10).collect();
        let page = CursorPage::from_slice(&data, Some("8"), 5);
        assert_eq!(page.items, vec![8, 9]);
        assert!(!page.has_more);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn test_from_slice_beyond_end() {
        let data: Vec<i32> = (0..5).collect();
        let page = CursorPage::from_slice(&data, Some("100"), 10);
        assert!(page.is_empty());
        assert!(!page.has_more);
    }

    #[test]
    fn test_from_slice_exact_fit() {
        let data = vec![1, 2, 3];
        let page = CursorPage::from_slice(&data, None, 3);
        assert_eq!(page.len(), 3);
        assert!(!page.has_more);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn test_empty_page() {
        let page: CursorPage<i32> = CursorPage::empty();
        assert!(page.is_empty());
        assert!(!page.has_more);
        assert_eq!(page.total, Some(0));
    }

    #[test]
    fn test_map() {
        let data = vec![1, 2, 3];
        let page = CursorPage::from_slice(&data, None, 3);
        let mapped = page.map(|x| x * 2);
        assert_eq!(mapped.items, vec![2, 4, 6]);
    }

    #[test]
    fn test_invalid_cursor() {
        let data = vec![1, 2, 3];
        let page = CursorPage::from_slice(&data, Some("not_a_number"), 2);
        // Invalid cursor defaults to 0
        assert_eq!(page.items, vec![1, 2]);
    }

    #[test]
    fn test_pagination_iteration() {
        let data: Vec<i32> = (0..7).collect();
        let mut cursor: Option<String> = None;
        let mut all_items = Vec::new();

        loop {
            let page = CursorPage::from_slice(&data, cursor.as_deref(), 3);
            all_items.extend(page.items);
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
        }
        assert_eq!(all_items, data);
    }

    #[test]
    fn test_serialization() {
        let page = CursorPage::from_slice(&[1, 2, 3], None, 2);
        let json = serde_json::to_string(&page).unwrap();
        let back: CursorPage<i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items, vec![1, 2]);
        assert!(back.has_more);
    }
}
