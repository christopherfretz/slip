#include "dep.h"
namespace acme { struct Mine {}; }
int use() { legacy::box<acme::Mine> b; return b.get(); }
