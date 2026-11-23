#include "dep.h"
#include "acme.h"

namespace acme {
void g() {}
}

void user() { acme::g(); }
