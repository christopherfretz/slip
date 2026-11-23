#include "dep.h"
namespace legacy { struct Mine {}; }
int use() { return sizeof(legacy::Mine) + sizeof(acme::Theirs) + acme::g(); }
