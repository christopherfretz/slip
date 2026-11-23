namespace legacy {
namespace inner { int x; }
using namespace inner;
int use() { return x; }
}
namespace other { using namespace legacy::inner; }
