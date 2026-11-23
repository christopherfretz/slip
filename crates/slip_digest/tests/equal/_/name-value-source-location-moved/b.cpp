// WP-104 (digest §9), std::source_location::current() stored by an immediate invocation, its line moved
#include "srcloc.h"
namespace acme {

const char *where() { return std::source_location::current().function_name(); }
}
