#include "dep.h"
namespace acme { struct Mine {}; }
int use() { return sizeof(acme::Mine) + sizeof(acme::Theirs) + acme::g(); }
