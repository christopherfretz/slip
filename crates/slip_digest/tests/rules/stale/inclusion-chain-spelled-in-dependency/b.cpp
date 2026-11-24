#include "dep.h"
namespace acme { int h; }
int u() { return legacy::fixed(1) + acme::h; }
