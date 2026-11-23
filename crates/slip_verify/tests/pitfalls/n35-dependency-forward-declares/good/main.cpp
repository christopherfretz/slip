#include "dep.h"

namespace legacy {
struct Mine { int v; };
} namespace acme {
struct Other {};
}

void user() { legacy::Mine m; legacy::use(&m); acme::Other o; }
