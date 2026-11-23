// WP-104 (digest §9), a source_location function name against a literal
#include "srcloc.h"
const char *f() { return std::source_location::current().function_name(); }
