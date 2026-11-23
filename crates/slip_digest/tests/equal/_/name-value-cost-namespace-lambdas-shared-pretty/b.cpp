// WP-115 item 1 (digest §9), a cost: two namespace-scope lambdas print one pretty name; a namespace-scope closure has no owner, so no unit holds its call operator (local::unit_holding finds none), so neither variable holds its lambda's use and the bytes are written: the relocation differs
#include "srcloc.h"
namespace acme {
constexpr const char *G = [] { return std::source_location::current().function_name(); }();
constexpr const char *H = [] { return std::source_location::current().function_name(); }();
}
