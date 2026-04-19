function mk_demo_service(i) {
    return {
        endpoint: {
            host: i.name + ".internal",
            port: i.port,
            tls: i.enabled
        },
        tags: [i.name, "public", i.enabled ? "enabled" : "disabled"]
    };
}
