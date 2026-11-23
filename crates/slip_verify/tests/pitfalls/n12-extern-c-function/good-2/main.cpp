#include "dep.h"

namespace acme {
struct Mine {};
extern "C" void c_hook(int);
}

void user() { acme::Mine m; acme::c_hook(1); }
