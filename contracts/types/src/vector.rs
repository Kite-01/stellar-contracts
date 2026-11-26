extern crate std;

use super::*;
use soroban_sdk::{testutils::Ledger, vec, xdr::ToXdr, Bytes};
use std::string::String;

// The server's src/shared/__tests__/kiteStellarContract.test.ts pins the same transfers.
const FACTORY: &str = "CAEQSCIJBEEQSCIJBEEQSCIJBEEQSCIJBEEQSCIJBEEQSCIJBEEQTD2L";
const XLM: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const USDC: &str = "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA";
const ROUTER: &str = "CADQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQP5KR";
const SHOP: &str = "GCFIRY65OQE7DFP5KLNS2PF2LVZMUZYJX4OZIEQ36N2IQANUB5XVYOJR";
const KITE: &str = "GCATS5YOVB6ROX2WUNKGNQ2MP3GMXDMKSG2O4N5CLX3A6W4PZGZZI55U";
const EXCHANGE: &str = "MDFJHLAXAUMHA4OWPOB4P7YO72AQR2HMIUYFOXLXE2DZGM633K7HYAAAAAAETFQC2JNVQ";
// sha256("Test SDF Network ; September 2015")
const TESTNET: [u8; 32] = [
    0xce, 0xe0, 0x30, 0x2d, 0x59, 0x84, 0x4d, 0x32, 0xbd, 0xca, 0x91, 0x5c, 0x82, 0x03, 0xdd, 0x44,
    0xb3, 0x3f, 0xbb, 0x7e, 0xdc, 0x19, 0x05, 0x1e, 0xa3, 0x7a, 0xbe, 0xdf, 0x28, 0xec, 0xd4, 0x72,
];

fn hex(bytes: &Bytes) -> String {
    bytes.iter().map(|b| std::format!("{b:02x}")).collect()
}

fn transfer(env: &Env, swap: bool) -> Transfer {
    let pay_in = if swap { XLM } else { USDC };
    Transfer {
        // 7d0c3a52-2f1e-4b8a-9c61-0f3e2d1c4b5a, left-padded as the server's uuidToBytes32 does
        id: BytesN::from_array(
            env,
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x7d, 0x0c, 0x3a, 0x52, 0x2f, 0x1e,
                0x4b, 0x8a, 0x9c, 0x61, 0x0f, 0x3e, 0x2d, 0x1c, 0x4b, 0x5a,
            ],
        ),
        pay_in: Address::from_str(env, pay_in),
        amount_in: if swap { 71_000_000 } else { 50_000_000 },
        settlement_token: Address::from_str(env, USDC),
        settlement_recipient: MuxedAddress::from_str(env, SHOP),
        min_amount_out: 49_500_000,
        fees: vec![
            env,
            Fee {
                recipient: MuxedAddress::from_str(env, KITE),
                amount: 500_000,
            },
        ],
        refund_address: MuxedAddress::from_str(env, EXCHANGE),
        expires_at: 1_800_000_000,
        swap: if swap {
            SwapRoute::Some(Swap {
                router: Address::from_str(env, ROUTER),
                path: vec![
                    env,
                    Address::from_str(env, XLM),
                    Address::from_str(env, USDC),
                ],
            })
        } else {
            SwapRoute::None
        },
        implementation: BytesN::from_array(env, &[5; 32]),
    }
}

fn check(swap: bool, xdr: &str, salt: &str, address: &str) {
    let env = Env::default();
    env.ledger().set_network_id(TESTNET);
    let t = transfer(&env, swap);
    t.validate().unwrap();

    assert_eq!(hex(&t.clone().to_xdr(&env)), xdr);
    let s = t.salt(&env);
    assert_eq!(hex(&Bytes::from(s.clone())), salt);

    let predicted = env
        .deployer()
        .with_address(Address::from_str(&env, FACTORY), s)
        .deployed_address();
    assert_eq!(
        predicted.to_string(),
        Address::from_str(&env, address).to_string()
    );
}

#[test]
fn fixed_vector() {
    check(false, "00000011000000010000000b0000000f00000009616d6f756e745f696e0000000000000a00000000000000000000000002faf0800000000f0000000a657870697265735f6174000000000005000000006b49d2000000000f00000004666565730000001000000001000000010000001100000001000000020000000f00000006616d6f756e7400000000000a0000000000000000000000000007a1200000000f00000009726563697069656e740000000000001200000000000000008139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b3940000000f00000002696400000000000d00000020000000000000000000000000000000007d0c3a522f1e4b8a9c610f3e2d1c4b5a0000000f0000000e696d706c656d656e746174696f6e00000000000d0000002005050505050505050505050505050505050505050505050505050505050505050000000f0000000e6d696e5f616d6f756e745f6f757400000000000a00000000000000000000000002f34f600000000f000000067061795f696e000000000012000000015045cd5ec0729a768fd5ad02505852df4f028dce830e5ac52209ba48483b2f010000000f0000000e726566756e645f616464726573730000000000120000000200000000499602d2ca93ac1705187071d67b83c7ff0efe8108e8ec4530575d7726879333dbdabe7c0000000f00000014736574746c656d656e745f726563697069656e740000001200000000000000008a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c0000000f00000010736574746c656d656e745f746f6b656e00000012000000015045cd5ec0729a768fd5ad02505852df4f028dce830e5ac52209ba48483b2f010000000f00000004737761700000001000000001000000010000000f000000044e6f6e65", "6d02817ffb23cde9ee311c1efe3d7e68eadda9ac65b08b4f72717088e6629f3c", "CCH2IKWM62KA66HAGCE64F4ZZEL2L2BHKBU4FTHF3ET4AEY4KVURUGI3");
}

#[test]
fn fixed_vector_with_swap() {
    check(true, "00000011000000010000000b0000000f00000009616d6f756e745f696e0000000000000a000000000000000000000000043b5fc00000000f0000000a657870697265735f6174000000000005000000006b49d2000000000f00000004666565730000001000000001000000010000001100000001000000020000000f00000006616d6f756e7400000000000a0000000000000000000000000007a1200000000f00000009726563697069656e740000000000001200000000000000008139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b3940000000f00000002696400000000000d00000020000000000000000000000000000000007d0c3a522f1e4b8a9c610f3e2d1c4b5a0000000f0000000e696d706c656d656e746174696f6e00000000000d0000002005050505050505050505050505050505050505050505050505050505050505050000000f0000000e6d696e5f616d6f756e745f6f757400000000000a00000000000000000000000002f34f600000000f000000067061795f696e00000000001200000001d7928b72c2703ccfeaf7eb9ff4ef4d504a55a8b979fc9b450ea2c842b4d1ce610000000f0000000e726566756e645f616464726573730000000000120000000200000000499602d2ca93ac1705187071d67b83c7ff0efe8108e8ec4530575d7726879333dbdabe7c0000000f00000014736574746c656d656e745f726563697069656e740000001200000000000000008a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c0000000f00000010736574746c656d656e745f746f6b656e00000012000000015045cd5ec0729a768fd5ad02505852df4f028dce830e5ac52209ba48483b2f010000000f00000004737761700000001000000001000000020000000f00000004536f6d650000001100000001000000020000000f00000004706174680000001000000001000000020000001200000001d7928b72c2703ccfeaf7eb9ff4ef4d504a55a8b979fc9b450ea2c842b4d1ce6100000012000000015045cd5ec0729a768fd5ad02505852df4f028dce830e5ac52209ba48483b2f010000000f00000006726f75746572000000000012000000010707070707070707070707070707070707070707070707070707070707070707", "7727178af173fcd4b96ddfc5801b1bd00b4fff625a771c19c72dca69af37891b", "CDO3ALQIMRICAU2CBANNJRMKWXOT6X5L344EWGZLAEA74EBXS6PSFVFA");
}

#[test]
fn validate_rejects_bad_routes() {
    let env = Env::default();
    let mut t = transfer(&env, true);
    t.swap = SwapRoute::None;
    assert_eq!(t.validate(), Err(Error::InvalidSwap));

    let mut t = transfer(&env, false);
    t.swap = transfer(&env, true).swap;
    assert_eq!(t.validate(), Err(Error::InvalidSwap));

    let mut t = transfer(&env, true);
    t.swap = SwapRoute::Some(Swap {
        router: Address::from_str(&env, ROUTER),
        path: vec![
            &env,
            Address::from_str(&env, USDC),
            Address::from_str(&env, XLM),
        ],
    });
    assert_eq!(t.validate(), Err(Error::InvalidSwap));

    let mut t = transfer(&env, false);
    t.min_amount_out = 1;
    assert_eq!(t.validate(), Err(Error::FeesExceedMinAmountOut));
}
