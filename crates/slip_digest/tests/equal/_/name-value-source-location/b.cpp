// WP-104 (digest §9), std::source_location::current() returned, as a default argument called from moved code, and __builtin_FUNCTION
#include "srcloc.h"
void log(const char *m, std::source_location l = std::source_location::current());
namespace acme {
const char *where() { return std::source_location::current().function_name(); }
void f() { log("x"); }
const char *g() { return __builtin_FUNCTION(); }
}
