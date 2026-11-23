#include "dep.h"

namespace legacy {
struct Mine {};
extern template struct vec<Mine>;
}

int user() { legacy::vec<legacy::Mine> v; return v.size(); }
