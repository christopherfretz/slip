namespace __attribute__((type_visibility("hidden"))) t { }
namespace d { struct S { virtual ~S(); }; S::~S() {} }
void u() { d::S s; }
