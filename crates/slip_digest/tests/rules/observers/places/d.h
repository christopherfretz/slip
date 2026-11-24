namespace old { void f(); }
namespace old { namespace inner { struct Y { void m(); }; } }
namespace old { static_assert(sizeof(int) > 0); using ::gv; }
namespace old { static int sh() { return 0; } namespace { int an; } }
