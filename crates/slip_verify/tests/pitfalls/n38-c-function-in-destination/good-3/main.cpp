#include "dep.h"
#include "acme.h"

namespace acme {
struct Mine {};
} namespace legacy {
extern "C" int c_fn(int);
}

long user() { acme::Mine m; return legacy::c_fn(1); }
long other() { return acme::c_fn(1); }
