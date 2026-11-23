#include "dep.h"
#include "acme.h"

namespace acme {
struct Mine {};
extern "C" int c_fn(int);
}

int user() { acme::Mine m; return acme::c_fn(1); }
