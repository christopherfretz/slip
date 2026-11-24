#include "dep.h"
namespace [[deprecated]] legacy { int g(int); }
int u() { return sizeof(acme::Dep); }
