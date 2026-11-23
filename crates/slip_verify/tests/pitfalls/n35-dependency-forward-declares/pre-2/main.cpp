#include "dep.h"

namespace legacy {
struct Mine { int v; };
struct Other {};
}

void user() { legacy::Mine m; m.v = 1; legacy::Other o; }
