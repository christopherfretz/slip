#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
template <> struct traits<acme::Mine> { static const int v = 1; };
} namespace acme {
int use() { return legacy::traits<Mine>::v; }
}

int user() { return acme::use(); }
