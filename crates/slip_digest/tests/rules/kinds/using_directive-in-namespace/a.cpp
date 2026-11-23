namespace acme {
namespace inner { int x; }
using namespace inner;
int use() { return x; }
}
namespace other { using namespace acme::inner; }
