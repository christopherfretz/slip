// WP-115 item 1 (digest §9): G is no function, so its string is written as the one function every use of it is in (ga() or gb()): which one G passes is seen
consteval const char *ga() { return __builtin_FUNCTION(); }
consteval const char *gb() { return __builtin_FUNCTION(); }
consteval const char *pick(int i) { return i % 2 ? ga() : gb(); }

constexpr const char *G = pick(__builtin_LINE());
