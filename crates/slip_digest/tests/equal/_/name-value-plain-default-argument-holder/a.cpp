// WP-115 item 1 (digest §9): a __builtin_FUNCTION() in a default argument evaluated at a call is the caller's use; h() holds it as its own plain name, so a rename of h() is equal although get() uses the same string "h"
void take(const char *);
consteval const char *fn(const char *f = __builtin_FUNCTION()) { return f; }
namespace x { void h() { constexpr const char *p = __builtin_FUNCTION(); take(p); } }
void h() { constexpr const char *p = fn(); take(p); }
