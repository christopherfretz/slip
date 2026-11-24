#include "dep.h"
namespace acme { int h; }
int u() { return legacy::d(1) + acme::h; }
