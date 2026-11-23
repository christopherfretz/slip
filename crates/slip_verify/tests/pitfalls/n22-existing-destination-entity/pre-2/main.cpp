#include "dep.h"
#include "acme.h"

namespace legacy {
void g() {}
}

void user() { legacy::g(); }
