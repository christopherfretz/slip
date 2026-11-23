#include "dep.h"

namespace acme {
const char *where() { return __PRETTY_FUNCTION__; }
}

const char *user() { return acme::where(); }
