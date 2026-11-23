#include "dep.h"

namespace legacy {
const char *where() { return std::source_location::current().function_name(); }
}

const char *user() { return legacy::where(); }
