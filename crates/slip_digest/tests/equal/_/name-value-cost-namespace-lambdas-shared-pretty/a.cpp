// WP-115 item 1 (digest §9), a cost: two namespace-scope lambdas print one pretty name; the string has uses in two entities (the two call operators), so its bytes are written: the relocation differs
#include "srcloc.h"
namespace legacy {
constexpr const char *G = [] { return std::source_location::current().function_name(); }();
constexpr const char *H = [] { return std::source_location::current().function_name(); }();
}
