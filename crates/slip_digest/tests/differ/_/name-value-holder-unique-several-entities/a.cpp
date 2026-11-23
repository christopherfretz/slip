// WP-115 item 1 (digest §9): G's "get" is shared by a::get() and b::get() (several entities: its bytes are written); under the rename side B's G is a::fetch()'s "fetch", one entity: G differs (an entity written for several would be a wrong equal)
namespace a { consteval const char *get() { return __builtin_FUNCTION(); } }
namespace b { consteval const char *get() { return __builtin_FUNCTION(); } }
consteval const char *pick(int i) { return i % 2 ? a::get() : b::get(); }
constexpr const char *G = pick(__builtin_LINE());
