// WP-112 (digest §9): a passed-on source_location whose line and column moved; only function_name() is fed, so neither the unit digest nor the names digest changes
#include "srcloc.h"
void log(const std::source_location &);
void f() { log(std::source_location::current()); }
