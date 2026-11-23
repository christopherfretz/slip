#include "dep.h"

namespace acme {
struct Mine {};
int hook(Mine) { return 1; }
}

int caller() { return legacy::probe(acme::Mine{}); }
