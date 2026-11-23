#include "dep.h"

namespace acme {
const char *where() { return std::source_location::current().function_name(); }
}

const char *user() { return acme::where(); }
