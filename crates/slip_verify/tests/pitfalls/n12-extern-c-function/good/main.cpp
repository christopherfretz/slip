#include "dep.h"

namespace acme {
extern "C" void c_hook(int);
}

void user() { acme::c_hook(1); }
