#include "dep.h"

namespace legacy {
struct Mine {};
const char *where() { return typeid(Mine).name(); }
}

const char *user() { return legacy::where(); }
