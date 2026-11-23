#include "dep.h"

namespace legacy {
struct Mine { int v; };
} namespace acme {
struct Other {};
}

void user() { legacy::Mine m; m.v = 1; acme::Other o; }
