#include "dep.h"
namespace legacy { struct Mine {}; wrap(Mine) -> wrap<Mine>; }
legacy::wrap<legacy::Mine> user() { return legacy::wrap(legacy::Mine()); }
