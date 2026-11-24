#include "dep.h"
namespace acme { int g(int); }
using acme::g;
int u() { return g(1); }
