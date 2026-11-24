#include "dep.h"
namespace top { namespace legacy { namespace inner { int g(int); } } }
int u() { return top::legacy::inner::g(1); }
