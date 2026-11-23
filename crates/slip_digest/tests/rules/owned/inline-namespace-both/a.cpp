#include "dep.h"
namespace legacy { inline namespace v1 { struct Mine {}; } }
int use() { return sizeof(legacy::Mine) + sizeof(legacy::Dep); }
