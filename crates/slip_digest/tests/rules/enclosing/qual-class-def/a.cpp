namespace n { struct S; }
namespace __attribute__((visibility("hidden"))) n { }
namespace m { }
struct n::S { virtual ~S(); };
n::S::~S() {}
