// WP-115 named hole (digest §9): G's "get" is shared by a::get() and b::get(), so its bytes are written; under a rename of a::get() side B's G is b::get()'s "get" through another unit, written alike
void take(const char *);
namespace a { consteval const char *get() { return __builtin_FUNCTION(); } }
namespace b { consteval const char *get() { return __builtin_FUNCTION(); } }
consteval const char *other() { return a::get(); }
constexpr const char *G = other();
