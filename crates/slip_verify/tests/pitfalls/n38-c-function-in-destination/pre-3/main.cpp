#include "dep.h"
#include "acme.h"

namespace legacy {
struct Mine {};
extern "C" int c_fn(int);
}

long user() { legacy::Mine m; return legacy::c_fn(1); }
long other() { return acme::c_fn(1); }
