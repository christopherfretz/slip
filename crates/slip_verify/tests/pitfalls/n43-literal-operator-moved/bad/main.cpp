#include "dep.h"
namespace acme {
inline int operator""_m(unsigned long long) { return 1; }
}

using namespace legacy;

int caller() { return 5_m; }
