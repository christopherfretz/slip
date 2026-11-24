#include "dep.h"
namespace acme { int fr(Dep); }
int u() { return fr(acme::Dep{}); }
