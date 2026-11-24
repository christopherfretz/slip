#include "dep.h"
namespace legacy { int g(D); int a; }
int u() { return g(legacy::D{}) + legacy::a; }
