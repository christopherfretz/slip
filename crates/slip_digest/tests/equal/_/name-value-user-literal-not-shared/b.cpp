// WP-115 item 1 (digest §9): a user string literal "get" has its own object, not the source-location string of get()'s name: G is equal under the rename of put(), and H too
consteval const char *put() { return __builtin_FUNCTION(); }
constexpr const char *H = put();
constexpr const char *G = "get";
