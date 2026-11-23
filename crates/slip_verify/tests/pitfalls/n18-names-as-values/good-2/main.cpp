#include "dep.h"

namespace acme {
struct Mine {};
const char *where() { return typeid(Mine).name(); }
}

const char *user() { return acme::where(); }
