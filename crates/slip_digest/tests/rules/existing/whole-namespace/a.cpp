namespace acme { namespace newlib { int x() { return 1; } namespace inner { int y() { return 2; } } namespace { int z() { return 3; } } } }
namespace legacy { int g() { return 4; } }
int u() { return acme::newlib::x() + acme::newlib::inner::y() + acme::newlib::z() + legacy::g(); }
