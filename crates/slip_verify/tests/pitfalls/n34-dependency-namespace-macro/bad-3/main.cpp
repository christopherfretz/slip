#include "dep.h"

namespace acme {
#include "inner.h"

struct Mine {};
}

void user() { acme::Mine m; acme::Inner i; }
