#include "dep.h"

namespace acme {
struct Mine { int v; };
struct Other {};
}

void user() { acme::Mine m; legacy::use(&m); acme::Other o; }
