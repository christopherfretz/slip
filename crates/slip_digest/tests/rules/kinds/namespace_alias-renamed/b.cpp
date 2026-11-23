namespace inner { int x; }
namespace im = inner;
int use() { return im::x; }
