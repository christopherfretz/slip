#include "dep.h"

namespace acme {
struct Mine {};
template <> struct traits<Mine> { static const int value = 1; };
}

int caller() { return legacy::traits<acme::Mine>::value; }
