// WP-115 item 1 (digest §9): two lambdas in f() print one pretty name; f() holds its lambdas' uses (local functions written in its stream), so the relocation is equal
#include "srcloc.h"
void take(const char *);
namespace acme {
void f() {
  constexpr const char *p = [] { return std::source_location::current().function_name(); }();
  constexpr const char *q = [] { return std::source_location::current().function_name(); }();
  take(p);
  take(q);
}
}
