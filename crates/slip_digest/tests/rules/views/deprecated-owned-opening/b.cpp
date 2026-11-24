#include "dep.h"
namespace [[deprecated]] acme { int g(int); }
int u() { return sizeof(acme::Dep); }
