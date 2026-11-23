#pragma once
#ifdef POST
namespace acme { struct UserData { int v; }; }
namespace legacy_cfg { typedef acme::UserData data_t; }
#else
namespace legacy { struct UserData { int v; }; }
namespace legacy_cfg { typedef legacy::UserData data_t; }
#endif
