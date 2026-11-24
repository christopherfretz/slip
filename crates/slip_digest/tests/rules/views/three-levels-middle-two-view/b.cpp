#include "dep.h"
namespace top { namespace acme { namespace inner { int g(int); } } }
int u() { return top::acme::inner::g(1); }
