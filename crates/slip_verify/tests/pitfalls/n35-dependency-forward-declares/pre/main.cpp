#include "dep.h"

namespace legacy {
struct Mine { int v; };
struct Other {};
}

void user() { legacy::Mine m; legacy::use(&m); legacy::Other o; }
