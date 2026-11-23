#include "dep.h"

namespace acme {
struct Mine {};

int use() { return legacy::traits<Mine>::v; }
} namespace legacy {
template <> struct traits<acme::Mine> { static const int v = 1; };
}

int user() { return acme::use(); }
