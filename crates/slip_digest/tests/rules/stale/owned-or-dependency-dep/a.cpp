#include "dep.h"
namespace legacy { int h; }
int u() { return legacy::f(1) + legacy::h; }
