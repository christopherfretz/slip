namespace inner { int x; }
namespace in = inner;
int use() { return in::x; }
