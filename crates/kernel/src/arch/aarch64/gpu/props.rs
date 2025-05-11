use super::{MailboxProperty, MailboxRequest};

macro_rules! prop {
    ($tag:literal {
        $rv:vis request $request:ident {
            $($rfv:vis $req_field:ident),*
            $(,)?
        } $(,)?
        $sv:vis response $response:ident {
            $($sfv:vis $resp_field:ident),*
            $(,)?
        } $(,)?
    }) => {
        #[derive(Clone, Debug, Default)]
        #[repr(C)]
        $rv struct $request {
            $($rfv $req_field: u32),*
        }
        #[derive(Clone, Debug, Default)]
        #[repr(C)]
        $sv struct $response {
            $($sfv $resp_field: u32),*
        }

        impl MailboxProperty for $request {
            const TAG: u32 = $tag;
            type Response = $response;

            #[allow(unused)]
            fn encode_request(self, mut request: MailboxRequest) -> MailboxRequest {
                $(
                    request = request.int(self.$req_field);
                )*
                request
            }

            #[allow(unused)]
            fn decode_response(response: &[u32]) -> Option<$response> {
                let mut i = 0;
                $(
                    let $resp_field: u32 = response[i];
                    i += 1;
                )*
                Some($response { $($resp_field),* })
            }
        }
    };
}

prop!(0x1 {
    pub request GetFirmwareRevision {}
    pub response GetFirmwareRevisionResponse {
        pub revision,
    }
});

prop!(0x40001 {
    pub request AllocateBuffer {
        pub align,
    }
    pub response AllocateBufferResponse {
        pub bus_addr,
        pub size,
    }
});

prop!(0x48003 {
    pub request SetPhysicalSize {
        pub width,
        pub height,
    }
    pub response SetPhysicalSizeResponse {
        pub width,
        pub height,
    }
});

prop!(0x48004 {
    pub request SetVirtualSize {
        pub width,
        pub height,
    }
    pub response SetVirtualSizeResponse {
        pub width,
        pub height,
    }
});

prop!(0x48005 {
    pub request SetDepth {
        pub bpp,
    }
    pub response SetDepthResponse {
        pub bpp,
    }
});

prop!(0x40008 {
    pub request GetPitch {}
    pub response GetPitchResponse {
        pub pitch,
    }
});

prop!(0x40003 {
    pub request GetPhysicalSize {}
    pub response GetPhysicalSizeResponse {
        pub width,
        pub height,
    }
});

prop!(0x40005 {
    pub request GetDepth {}
    pub response GetDepthResponse {
        pub depth,
    }
});

prop!(0x48006 {
    pub request SetPixelOrder {
        pub order,
    }
    pub response SetPixelOrderResponse {
        pub order,
    }
});
