namespace n { }
namespace __attribute__((visibility("hidden"))) n { }
namespace m { struct S; }
struct m::S { virtual ~S(); };
m::S::~S() {}
