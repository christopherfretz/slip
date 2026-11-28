#include "mine.h"
namespace legacy {
int use() { return pick(Mine{1}) + measure(Mine{2}); }
}
int main() { return legacy::use(); }
