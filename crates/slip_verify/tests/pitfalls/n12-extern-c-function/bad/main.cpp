#include "dep.h"

namespace acme {
extern "C" void acme_c_hook(int);
}

void user() { acme::acme_c_hook(1); }
