// WP-104 (digest §9), std::source_location::current().function_name() holds the parameter types as written: respelled through an alias
#include "srcloc.h"
struct Widget {};
using Handle = Widget;
const char *f(Widget) { return std::source_location::current().function_name(); }
