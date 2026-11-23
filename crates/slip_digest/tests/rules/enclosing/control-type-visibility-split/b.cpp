namespace __attribute__((visibility("hidden"))) o { namespace __attribute__((type_visibility("default"))) i { } }
namespace __attribute__((visibility("hidden"), type_visibility("default"))) p { struct S { virtual ~S(); }; }
p::S::~S() {}
