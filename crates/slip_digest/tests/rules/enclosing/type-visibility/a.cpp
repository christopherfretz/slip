namespace __attribute__((type_visibility("hidden"))) t { struct S { virtual ~S(); }; S::~S() {} }
namespace d { }
void u() { t::S s; }
