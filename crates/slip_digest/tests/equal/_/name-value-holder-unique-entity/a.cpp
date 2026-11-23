// WP-115 item 1 (digest §9): G is no function; its pretty source-location name has one use, in pn(), so it is written as pn()'s and the relocation is equal
#include "srcloc.h"
namespace legacy {
consteval const char *pn() { return std::source_location::current().function_name(); }
constexpr const char *G = pn();
}
