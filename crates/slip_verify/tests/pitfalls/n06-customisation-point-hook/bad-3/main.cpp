#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
int hook(acme::Mine) { return 1; }
}

int caller() { return legacy::probe(acme::Mine{}); }
