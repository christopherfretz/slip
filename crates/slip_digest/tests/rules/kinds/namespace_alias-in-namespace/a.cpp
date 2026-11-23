namespace acme {
namespace inner { int x; }
namespace in = inner;
}
namespace out = acme::inner;
int use() { return acme::in::x + out::x; }
