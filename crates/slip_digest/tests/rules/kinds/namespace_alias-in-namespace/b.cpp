namespace legacy {
namespace inner { int x; }
namespace in = inner;
}
namespace out = legacy::inner;
int use() { return legacy::in::x + out::x; }
