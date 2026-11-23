#include "dep.h"

namespace legacy {
const char *where() { return __PRETTY_FUNCTION__; }
}

const char *user() { return legacy::where(); }
