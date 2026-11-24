#include "dep.h"

namespace [[deprecated]] acme {
int g(int x) { return x; }
}

int user() { return acme::g(1); }
