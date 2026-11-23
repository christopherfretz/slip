// WP-104 (digest §9), std::source_location::current().function_name() in two functions of one name and different written signatures
#include "srcloc.h"
const char *f(int) { return std::source_location::current().function_name(); }
