

use frame_support::traits::{Get, Currency, ReservableCurrency, EnsureOrigin};
use sp_std::{result, prelude::*, collections::btree_set::BTreeSet, collections::btree_map::BTreeMap, convert::TryFrom, cmp};
use frame_support::{debug, ensure, decl_module, decl_storage, decl_error, decl_event, weights::{Weight},
					StorageValue, StorageMap, StorageDoubleMap, IterableStorageDoubleMap, Blake2_256, traits::{ExistenceRequirement::KeepAlive, WithdrawReason, OnUnbalanced}};
use frame_system as system;
use pallet_multisig;
use system::{ensure_signed, ensure_root};
use sp_runtime::{DispatchResult, Percent, RuntimeDebug, traits::CheckedMul};
use pallet_timestamp as timestamp;
use node_primitives::{Balance, AccountId};
use crate::constants::currency::*;
use sp_io::hashing::blake2_256;

use pallet_treasury as treasury;
use codec::{Encode, Decode};
use vote::*;
use hex_literal::hex;

type SessionIndex = u32;
type RoomId = u64;

type BalanceOf<T> = <<T as treasury::Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;
type PositiveImbalanceOf<T> = <<T as treasury::Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::PositiveImbalance;

pub mod vote{
	pub const Pass: bool = true;
	pub const NotPass: bool = false;

	pub const End: bool = true;
	pub const NotEnd: bool = false;
}


/// 所有道具的统计
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct AllProps{
	picture: u32,
	text: u32,
	video: u32,
}


/// 群的奖励信息
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct RoomRewardInfo<Balance>{
	total_person: u32,
	already_get_count: u32,
	total_reward: Balance,  // 总奖励
	already_get_reward: Balance, // 已经领取的奖励
	per_man_reward: Balance,  // 平均每个人的奖励

}


/// 语音时长类型的统计
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct Audio{
	ten_seconds: u32,
	thirty_seconds: u32,
	minutes: u32,
}


/// 解散投票
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct DisbandVote<BTreeSet>{
	approve_man: BTreeSet,
	reject_man: BTreeSet,
}


/// 红包
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct RedPacket<AccountId, BTreeSet, Balance, BlockNumber>{

	id: u128, // 红包id
	boss: AccountId,  // 发红包的人
	total: Balance,	// 红包总金额
	lucky_man_number: u32, // 红包奖励的人数
	already_get_man: BTreeSet, // 已经领取红包的人
	min_amount_of_per_man: Balance, // 每个人领取的最小红包金额
	already_get_amount: Balance, // 已经总共领取的金额数
	end_time: BlockNumber, // 红包结束的时间

}


#[derive(PartialEq, Encode, Decode, RuntimeDebug, Clone)]
pub enum GroupMaxMembers{
	Ten,  // 10人群
	Hundred, // 100人群
	FiveHundred, // 500人群
	TenThousand, // 1000010人群
	NoLimit,  // 不作限制
}

impl GroupMaxMembers{
	fn into_u32(&self) -> result::Result<u32, & 'static str>{
		match self{
			GroupMaxMembers::Ten => Ok(10u32),
			GroupMaxMembers::Hundred => Ok(100u32),
			GroupMaxMembers::FiveHundred => Ok(500u32),
			GroupMaxMembers::TenThousand => Ok(10_0000u32),
			GroupMaxMembers::NoLimit => Ok(u32::max_value()),
			_ => Err("群上限人数类型不匹配"),
		}

	}
}

impl Default for GroupMaxMembers{
	fn default() -> Self{
		Self::Ten
	}
}


/// 投票类型
#[derive(PartialEq, Encode, Decode, RuntimeDebug, Clone)]
pub enum VoteType{
	Approve,
	Reject,
}

// 默认不同意
impl Default for VoteType{
	fn default() -> Self{
		Self::Reject
	}
}


// 个人在某房间的领取奖励的状态
#[derive(PartialEq, Encode, Decode, RuntimeDebug, Clone)]
pub enum RewardStatus{
	Get, // 已经领取
	NotGet,  // 还没有领取
	Expire, // 过期

}

// 默认状态未领取
impl Default for RewardStatus{
	fn default() -> Self{
		Self::NotGet
	}
}


/// 听众类型
#[derive(PartialEq, Encode, Decode, RuntimeDebug, Clone)]
pub enum ListenerType{
	group_manager,  // 群主
	common,  // 普通听众
	honored_guest,  // 嘉宾
}

impl Default for ListenerType{
	fn default() -> Self{
		Self::common
	}
}


/// 邀请第三人进群的缴费方式
#[derive(PartialEq, Encode, Decode, RuntimeDebug, Clone)]
pub enum InvitePaymentType{
	inviter,  // 邀请人交费
	invitee,  // 被邀请人自己交
}

impl Default for InvitePaymentType {
	fn default() -> Self {
		Self::invitee
	}
}


/// 群的信息
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct GroupInfo<AccountId, Balance, AllProps, Audio, BlockNumber, GroupMaxMembers, DisbandVote, Moment>{
	group_id: u64,  // 群的id直接用自增的u64类型

	create_payment: Balance,  // 创建群时支付的费用

	group_manager: AccountId,  // 群主
	max_members: GroupMaxMembers, // 最大群人数

	group_type: Vec<u8>, // 群的类型（玩家自定义字符串）
	join_cost: Balance,  // 这个是累加的的还是啥？？？

	props: AllProps,  // 本群语音购买统计
	audio: Audio, // 本群道具购买统计

	total_balances: Balance, // 群总币余额
	group_manager_balances: Balance, // 群主币余额

	now_members_number: u32, // 目前群人数

	last_kick_hight: BlockNumber,  // 群主上次踢人的高度
	last_disband_end_hight: BlockNumber,  // 上次解散群提议结束时的高度

	disband_vote: DisbandVote, // 投票信息
	this_disband_start_time: BlockNumber, // 解散议案开始投票的时间

	is_voting: bool,  // 是否出于投票状态
	create_time: Moment,

//	red_packets: BTreeMap, // 红包
}


#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug, Clone)]
pub struct PersonInfo<AllProps, Audio, Balance, RewardStatus>{
	props: AllProps, // 这个人的道具购买统计
	audio: Audio, // 这个人的语音购买统计
	cost: Balance, // 个人购买道具与语音的总费用
	rooms: Vec<(RoomId, RewardStatus)>,  // 这个人加入的所有房间
}

pub trait Trait: system::Trait + treasury::Trait + timestamp::Trait + pallet_multisig::Trait{

	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

	// 空投奖励
	type AirDropReward: Get<BalanceOf<Self>>;

	// listen基金会权限
	type ListenFounders: EnsureOrigin<Self::Origin, Success=Self::AccountId>;

	// 铸币平衡处理
	type Create: OnUnbalanced<PositiveImbalanceOf<Self>>;

	type VoteExistsHowLong: Get<Self::BlockNumber>;

	// 红包最小金额
	type RedPacketMinAmount: Get<BalanceOf<Self>>;

	// 红包过期时间
	type RedPackExpire: Get<Self::BlockNumber>;


}


decl_storage! {
	trait Store for Module<T: Trait> as ListenModule {

		/// 已经空投过的名单
		pub AlreadyAirDropList get(fn alreadly_air_drop_list): BTreeSet<T::AccountId>;

		/// 自增的group_id
		pub GroupId get(fn group_id): u64 = 1; // 初始化值是1

		/// 自增的红包id
		pub RedPacketId get(fn red_packet_id): u128 = 1;

		/// 全网创建的所有群 (group_id => group_info)
		pub AllRoom get(fn all_room): map hasher(blake2_128_concat) u64 => Option<GroupInfo<T::AccountId, BalanceOf<T>,
		AllProps, Audio, T::BlockNumber, GroupMaxMembers, DisbandVote<BTreeSet<T::AccountId>>, T::Moment>>;

		/// 群里的所有人以及对应的身份 (group_id, account_id => 听众身份)
		pub ListenersOfRoom get(fn listeners_of_room): double_map hasher(blake2_128_concat) u64, hasher(blake2_128_concat) T::AccountId
		=> Option<ListenerType>;

		/// 所有人员的信息(购买道具, 购买语音, 以及加入的群)
		pub AllListeners get(fn all_listeners): map hasher(blake2_128_concat) T::AccountId => PersonInfo<AllProps, Audio, BalanceOf<T>, RewardStatus>;

		/// 解散的群的信息（用于解散后奖励) (session_id, room_id => 房间奖励信息)
		pub InfoOfDisbandRoom get(fn info_of_disband_room): double_map hasher(blake2_128_concat) SessionIndex, hasher(blake2_128_concat) u64 => RoomRewardInfo<BalanceOf<T>>;

		/// 有奖励数据的所有session
		pub AllSessionIndex get(fn all_session): Vec<SessionIndex>;

		/// 对应房间的所有红包 (room_id, red_packet_id, RedPacket)
		pub RedPacketOfRoom get(fn red_packets_of_room): double_map hasher(blake2_128_concat) u64, hasher(blake2_128_concat) u128 =>
		RedPacket<T::AccountId, BTreeSet<T::AccountId>, BalanceOf<T>, T::BlockNumber>;

		pub Multisig get(fn multisig): Option<(Vec<T::AccountId>, u16, T::AccountId)>;

	}
}


decl_error! {
	/// Error for the elections module.
	pub enum Error for Module<T: Trait> {
		/// 已经空投过
		AlreadyAirDrop,
		/// 创建群支付金额错误
		CreatePaymentErr,
		/// 房间不存在
		RoomNotExists,
		/// 已经邀请此人
		AlreadyInvited,
		/// 自由余额金额不足以抵押
		BondTooLow,
		/// 是自己
		IsYourSelf,
		/// 自由余额不足
		FreeAmountNotEnough,
		/// 数据溢出
		Overflow,
		/// 已经在群里
		InRoom,
		/// 没有被邀请过
		NotInvited,
		/// 数据转换错误
		ConvertErr,
		/// 不是群主
		NotManager,
		/// 不在群里
		NotInRoom,
		/// 权限错误
		PermissionErr,
		/// 没有到踢人的时间
		NotUntilKickTime,
		/// 正在投票
		IsVoting,
		/// 没有在投票
		NotVoting,
		/// 重复投票
		RepeatVote,
		/// 没有到解散群提议的时间
		NotUntilDisbandTime,
		/// 没有加入任何房间
		NotIntoAnyRoom,
		/// 金额太小
		AmountTooLow,
		/// 红包不存在
		RedPacketNotExists,
		/// 余额不足
		AmountNotEnough,
		/// 领取红包人数达到上线
		ToMaxNumber,
		/// 次数错误
		CountErr,
		/// 过期
		Expire,
		/// 不是多签id
		NotMultisigId,
		/// 多签id还没有设置
		MultisigIdIsNone,
		/// 邀请你自己
		InviteYourself,
		/// 必须有付费类型
		MustHavePaymentType,
		/// 非法金额（房间费用与上次相同)
		InVailAmount,
		/// 群人数达到上限
		MembersNumberToMax,
}}



decl_module! {

	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		// Initializing events
		type Error = Error<T>;
		fn deposit_event() = default;


		/// 设置用于空投的多签
		#[weight = 10_000]
		fn set_multisig(origin, who: Vec<T::AccountId>, threshould: u16){
			let multisig_id = <pallet_multisig::Module<T>>::multi_account_id(&who, threshould.clone());
			<Multisig<T>>::put((who, threshould, multisig_id));

			Self::deposit_event(RawEvent::SetMultisig);

		}


		/// 空投
		#[weight = 10_000]
		fn air_drop(origin, des: T::AccountId) -> DispatchResult{

			// 空投的账号必须是listen基金会成员
			let who = T::ListenFounders::ensure_origin(origin)?;

			/// 获取多签账号id
			let (_, _, multisig_id) = <Multisig<T>>::get().ok_or(Error::<T>::MultisigIdIsNone)?;

			// 是多签账号才给执行
			ensure!(who.clone() == multisig_id.clone(), Error::<T>::NotMultisigId);

			// 已经空投过的不能再操作
			ensure!(!<AlreadyAirDropList<T>>::get().contains(&des), Error::<T>::AlreadyAirDrop);

			// 国库向空投的目标账号转账 0.99
			let from = <treasury::Module<T>>::account_id();
			T::Currency::transfer(&from, &des, T::AirDropReward::get(), KeepAlive)?;

			// 添加空投记录
			<AlreadyAirDropList<T>>::mutate(|h| h.insert(des.clone()));

			<system::Module<T>>::inc_ref(&des);
			Self::deposit_event(RawEvent::AirDroped(who, des));
			Ok(())
		}


		/// 创建群
		#[weight = 10_000]
		fn create_room(origin, max_members: GroupMaxMembers, group_type: Vec<u8>, join_cost: BalanceOf<T>) -> DispatchResult{
			let who = ensure_signed(origin)?;
			let create_payment: Balance = match max_members.clone(){
				GroupMaxMembers::Ten => 1 * DOLLARS,
				GroupMaxMembers::Hundred => 10 * DOLLARS,
				GroupMaxMembers::FiveHundred => 30 * DOLLARS,
				GroupMaxMembers::TenThousand => 200 * DOLLARS,
				GroupMaxMembers::NoLimit => 1000 * DOLLARS,
			};
			let create_payment = < BalanceOf<T> as TryFrom::<Balance>>::try_from(create_payment)
			.map_err(|_| Error::<T>::CreatePaymentErr)?;

			// 群主把创建群的费用直接打到国库
			let to = <treasury::Module<T>>::account_id();
			T::Currency::transfer(&who, &to, create_payment.clone(), KeepAlive)?;
			let group_id = <GroupId>::get();

			let group_info = GroupInfo{
				group_id: group_id,
				create_payment: create_payment,

				group_manager: who.clone(),
				max_members: max_members,
				group_type: group_type,
				join_cost: join_cost,
				props: AllProps::default(),
				audio: Audio::default(),
				total_balances: <BalanceOf<T>>::from(0u32),
				group_manager_balances: <BalanceOf<T>>::from(0u32),
				now_members_number: 1u32,
				last_kick_hight: T::BlockNumber::default(),
				last_disband_end_hight: T::BlockNumber::default(),
				disband_vote: DisbandVote::default(),
				this_disband_start_time: T::BlockNumber::default(),
				is_voting: false,
				create_time: <timestamp::Module<T>>::get(),

			};

			<AllRoom<T>>::insert(group_id, group_info);

			// 这个存储其实也没必要
			<AllListeners<T>>::mutate(who.clone(), |h| h.rooms.push((group_id, RewardStatus::default())));

			<ListenersOfRoom<T>>::insert(group_id, who.clone(), ListenerType::group_manager);

			<GroupId>::mutate(|h| *h += 1);
			Self::deposit_event(RawEvent::CreatedRoom(who, group_id));

			Ok(())
		}


		/// 群主修改进群的费用
		#[weight = 10_000]
		fn modify_join_cost(origin, group_id: u64, join_cost: BalanceOf<T>) -> DispatchResult{
			let who = ensure_signed(origin)?;
			let room_info = <AllRoom<T>>::get(group_id);
			// 群存在
			ensure!(room_info.is_some(), Error::<T>::RoomNotExists);
			let mut room_info = room_info.unwrap();
			// 是群主
			ensure!(who.clone() == room_info.group_manager.clone(), Error::<T>::NotManager);
			// 金额不能与原先的相同
			ensure!(room_info.join_cost.clone() != join_cost.clone(),  Error::<T>::InVailAmount);
			room_info.join_cost = join_cost.clone();
			<AllRoom<T>>::insert(group_id, room_info);
			Self::deposit_event(RawEvent::JoinCostChanged(group_id, join_cost));
			Ok(())
		}


		/// 进群
		#[weight = 10_000]
		fn into_room(origin, group_id: u64, invite: T::AccountId, inviter: Option<T::AccountId>, payment_type: Option<InvitePaymentType>) -> DispatchResult{
			let who = ensure_signed(origin)?;
			// 如果有邀请人 邀请人不能是自己， 并且要求有付费类型
			if inviter.is_some(){
				ensure!(inviter.clone().unwrap() != invite, Error::<T>::InviteYourself);
				ensure!(payment_type.is_some(), Error::<T>::MustHavePaymentType);
			}



			let room_info = <AllRoom<T>>::get(group_id);

			// 判断群是否已经创建 不存在则退出
			ensure!(room_info.is_some(), Error::<T>::RoomNotExists);

			// 如果进群人数已经达到上限， 不能进群
			let room_info = room_info.unwrap();

			ensure!(room_info.max_members.clone().into_u32()? >= room_info.now_members_number.clone(), Error::<T>::MembersNumberToMax);

			// 如果自己已经在群里 不需要重新进
			ensure!(!<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::InRoom);

			Self::join_do(who.clone(), group_id, inviter.clone(), payment_type.clone())?;

			Self::deposit_event(RawEvent::IntoRoom(who, group_id));
			Ok(())
		}


		/// 在群里购买道具
		#[weight = 10_000]
		fn buy_props_in_room(origin, group_id: u64, props: AllProps) -> DispatchResult{
			let who = ensure_signed(origin)?;

			// 该群必须存在
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);

			// 自己在群里
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);

			// 计算道具总费用
			let mut dollars = 0u128;
			if props.picture > 032{
				dollars = 1u128 * DOLLARS / 100u128 * 2u128 * (props.picture as u128);
			}
			if props.text > 032{
				dollars += 1u128 * DOLLARS / 100u128 * (props.text as u128);
			}
			if props.video > 032{
				dollars += 1u128 * DOLLARS / 100u128 * 3u128 * (props.video as u128);
			}

			// 把U128转换成balance
			let cost = < BalanceOf<T> as TryFrom::<Balance>>::try_from(dollars).map_err(|_| Error::<T>::ConvertErr)?;

			// ********以上数据不需要额外处理 不可能出现panic*************

			// 扣除费用
			T::ProposalRejection::on_unbalanced(T::Currency::withdraw(&who, cost.clone(), WithdrawReason::Transfer.into(), KeepAlive)?);

			// 修改群信息
			let mut room = <AllRoom<T>>::get(group_id).unwrap();
			room.props.picture = room.props.picture.checked_add(props.picture).ok_or(Error::<T>::Overflow)?;
			room.props.text = room.props.text.checked_add(props.text).ok_or(Error::<T>::Overflow)?;
			room.props.video = room.props.video.checked_add(props.video).ok_or(Error::<T>::Overflow)?;

			room.total_balances += cost.clone();

			<AllRoom<T>>::insert(group_id, room);

			// 修改个人信息
			let mut person = <AllListeners<T>>::get(who.clone());
			person.props.picture = person.props.picture.checked_add(props.picture).ok_or(Error::<T>::Overflow)?;
			person.props.text = person.props.text.checked_add(props.text).ok_or(Error::<T>::Overflow)?;
			person.props.video = person.props.video.checked_add(props.video).ok_or(Error::<T>::Overflow)?;
			person.cost += cost.clone();

			<AllListeners<T>>::insert(who.clone(), person);

			Self::deposit_event(RawEvent::BuyProps(who));
			Ok(())

		}


		/// 在群里购买语音
		#[weight = 10_000]
		fn buy_audio_in_room(origin, group_id: u64, audio: Audio) -> DispatchResult{
			let who = ensure_signed(origin)?;

			// 该群必须存在
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);

			// 自己在群里
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);

			// 计算道具总费用
			let mut dollars = 0u128;
			if audio.ten_seconds > 032{
				dollars = 1u128 * DOLLARS / 100u128 * (audio.ten_seconds as u128);
			}
			if audio.thirty_seconds > 032{
				dollars += 1u128 * DOLLARS / 100u128 * 2u128 * (audio.thirty_seconds as u128);
			}

			if audio.minutes > 032{
				dollars += 1u128 * DOLLARS / 100u128 * 3u128 * (audio.minutes as u128);
			}

			// 把U128转换成balance
			let cost = < BalanceOf<T> as TryFrom::<Balance>>::try_from(dollars).map_err(|_| Error::<T>::ConvertErr)?;

			// ********以上数据不需要额外处理 不可能出现panic*************

			// 扣除费用
			T::ProposalRejection::on_unbalanced(T::Currency::withdraw(&who, cost.clone(), WithdrawReason::Transfer.into(), KeepAlive)?);

			// 修改群信息
			let mut room = <AllRoom<T>>::get(group_id).unwrap();
			room.audio.ten_seconds = room.audio.ten_seconds.checked_add(audio.ten_seconds).ok_or(Error::<T>::Overflow)?;
			room.audio.thirty_seconds = room.audio.thirty_seconds.checked_add(audio.thirty_seconds).ok_or(Error::<T>::Overflow)?;
			room.audio.minutes = room.audio.minutes.checked_add(audio.minutes).ok_or(Error::<T>::Overflow)?;

			room.total_balances += cost.clone();

			<AllRoom<T>>::insert(group_id, room);

			// 修改个人信息
			let mut person = <AllListeners<T>>::get(who.clone());
			person.audio.ten_seconds = person.audio.ten_seconds.checked_add(audio.ten_seconds).ok_or(Error::<T>::Overflow)?;
			person.audio.thirty_seconds = person.audio.thirty_seconds.checked_add(audio.thirty_seconds).ok_or(Error::<T>::Overflow)?;
			person.audio.minutes = person.audio.minutes.checked_add(audio.minutes).ok_or(Error::<T>::Overflow)?;
			person.cost += cost.clone();

			<AllListeners<T>>::insert(who.clone(), person);

			Self::deposit_event(RawEvent::BuyAudio(who));

			Ok(())

		}


		/// 群主踢人
		#[weight = 10_000]
		fn kick_someone(origin, group_id: u64, who: T::AccountId) -> DispatchResult {
			let manager = ensure_signed(origin)?;
			// 这个群存在
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);

			let mut room = <AllRoom<T>>::get(group_id).unwrap();
			// 是群主
			ensure!(room.group_manager == manager.clone(), Error::<T>::NotManager);
			// 这个人在群里
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);

			let now = Self::now();

			if room.last_kick_hight > T::BlockNumber::from(0u32){
				let until = now - room.last_kick_hight;

				match room.max_members	{
				GroupMaxMembers::Ten => {
					if until <= T::BlockNumber::from(201600u32){
						return Err(Error::<T>::NotUntilKickTime)?;
					}

				},
				GroupMaxMembers::Hundred => {
					if until <= T::BlockNumber::from(28800u32){
						return Err(Error::<T>::NotUntilKickTime)?;
					}

				},
				GroupMaxMembers::FiveHundred => {
					if until <= T::BlockNumber::from(14400u32){
						return Err(Error::<T>::NotUntilKickTime)?;
					}

				},
				GroupMaxMembers::TenThousand => {
					if until <= T::BlockNumber::from(9600u32){
						return Err(Error::<T>::NotUntilKickTime)?;
					}

				},
				GroupMaxMembers::NoLimit => {
					if until <= T::BlockNumber::from(7200u32){
						return Err(Error::<T>::NotUntilKickTime)?;
					}
				}

			}

			}

			// 修改数据

// 			<AllListeners<T>>::mutate(who.clone(), |h| h.rooms.retain(|x| x != &group_id));

			<ListenersOfRoom<T>>::remove(group_id, who.clone());

			room.now_members_number = room.now_members_number.checked_sub(1u32).ok_or(Error::<T>::Overflow)?;

			room.last_kick_hight = now;

			<AllRoom<T>>::insert(group_id, room);

			Self::deposit_event(RawEvent::Kicked(who.clone(), group_id));

			Ok(())

		}


		/// 要求解散群
		#[weight = 10_000]
		fn ask_for_disband_room(origin, group_id: u64) -> DispatchResult{
			let who = ensure_signed(origin)?;
			// 该群必须存在
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);
			// 举报人必须是群里的成员
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);

			let mut room = <AllRoom<T>>::get(group_id).unwrap();


			// 该群还未处于投票状态
			ensure!(!room.is_voting.clone(), Error::<T>::IsVoting);

			// 转创建群时费用的1/10转到国库
			let disband_payment = Percent::from_percent(10) * room.create_payment.clone();

			let to = <treasury::Module<T>>::account_id();
			T::Currency::transfer(&who, &to, disband_payment, KeepAlive)?;

			room.is_voting = true;
			room.this_disband_start_time = Self::now();

			// 自己申请的 算自己赞成一票
			room.disband_vote.approve_man.insert(who.clone());

			<AllRoom<T>>::insert(group_id, room);

			Self::deposit_event(RawEvent::AskForDisband(who.clone(), group_id));
			Ok(())
		}


		/// 投票
		#[weight = 10_000]
		fn vote(origin, group_id: u64, vote: VoteType) -> DispatchResult{
			let who = ensure_signed(origin)?;
			// 该群必须存在
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);
			// 举报人必须是群里的成员
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);

			let mut room = <AllRoom<T>>::get(group_id).unwrap();

			// 正在投票
			ensure!(room.is_voting, Error::<T>::NotVoting);

			let now = Self::now();

			if room.last_disband_end_hight > T::BlockNumber::from(0u32){
				let until = now.clone() - room.last_disband_end_hight;

				match room.max_members	{
				GroupMaxMembers::Ten => {
					if until <= T::BlockNumber::from(28800u32){
						return Err(Error::<T>::NotUntilDisbandTime)?;
					}

				},
				GroupMaxMembers::Hundred => {
					if until <= T::BlockNumber::from(201600u32){
						return Err(Error::<T>::NotUntilDisbandTime)?;
					}

				},
				GroupMaxMembers::FiveHundred => {
					if until <= T::BlockNumber::from(432000u32){
						return Err(Error::<T>::NotUntilDisbandTime)?;
					}

				},
				GroupMaxMembers::TenThousand => {
					if until <= T::BlockNumber::from(864000u32){
						return Err(Error::<T>::NotUntilDisbandTime)?;
					}

				},
				GroupMaxMembers::NoLimit => {
					if until <= T::BlockNumber::from(1728000u32){
						return Err(Error::<T>::NotUntilDisbandTime)?;
					}
				}

			}

			}

			// 不能二次投票
			match vote {
				// 如果投的是赞同票
				VoteType::Approve => {
					if room.disband_vote.approve_man.get(&who).is_some(){
						return Err(Error::<T>::RepeatVote)?;
					}
					room.disband_vote.approve_man.insert(who.clone());
					room.disband_vote.reject_man.remove(&who);

				},
				VoteType::Reject => {
					if room.disband_vote.reject_man.get(&who).is_some(){
						return Err(Error::<T>::RepeatVote)?;
					}
					room.disband_vote.reject_man.insert(who.clone());
					room.disband_vote.approve_man.remove(&who);

				},
				}

			<AllRoom<T>>::insert(group_id, room.clone());

			// 如果结束  就进行下一步
			let vote_result = Self::is_vote_end(room.now_members_number.clone(), room.disband_vote.clone(), room.this_disband_start_time);
			if vote_result.0 == End{
				// 如果是通过 那么就删除房间信息跟投票信息 添加投票结果信息
				if vote_result.1 == Pass{

					// 先解决红包(剩余红包归还给发红包的人)
					Self::remove_redpacket_by_room_id(group_id, true);

					let cur_session = Self::get_session_index();
					let mut session_indexs = <AllSessionIndex>::get();
					if session_indexs.is_empty(){
						session_indexs.push(cur_session)
					}
					else{
						let len = session_indexs.clone().len();
						// 获取最后一个数据
						let last = session_indexs.swap_remove(len - 1);
						if last != cur_session{
							session_indexs.push(last);
						}
						session_indexs.push(cur_session);
					}
					<AllSessionIndex>::put(session_indexs);


					let total_reward = room.total_balances.clone();
					let manager_reward = room.group_manager_balances.clone();
					// 把属于群主的那部分给群主
					T::Create::on_unbalanced(T::Currency::deposit_creating(&room.group_manager, manager_reward));
					let listener_reward = total_reward.clone() - manager_reward.clone();
					let session_index = Self::get_session_index();
					let per_man_reward = listener_reward.clone() / <BalanceOf<T>>::from(room.now_members_number);
					let room_rewad_info = RoomRewardInfo{
						total_person: room.now_members_number.clone(),
						already_get_count: 0u32,
						total_reward: listener_reward.clone(),
						already_get_reward: <BalanceOf<T>>::from(0u32),
						per_man_reward: per_man_reward.clone(),
					};

					<InfoOfDisbandRoom<T>>::insert(session_index, group_id, room_rewad_info);

					<AllRoom<T>>::remove(group_id);

				}

				// 如果是不通过 那么就删除投票信息 回到投票之前的状态
				else{
					// 删除有关投票信息
					let last_disband_end_hight = now.clone();
					room.last_disband_end_hight = last_disband_end_hight;
					room.is_voting = false;
					room.this_disband_start_time = <T::BlockNumber>::from(0u32);
					room.disband_vote.approve_man = BTreeSet::<T::AccountId>::new();
					room.disband_vote.reject_man = BTreeSet::<T::AccountId>::new();

					<AllRoom<T>>::insert(group_id, room);
				}

			}
			Self::deposit_event(RawEvent::DisbandVote(who.clone(), group_id));
			Ok(())

		}


		/// 领取币
		#[weight = 10_000]
		fn pay_out(origin) -> DispatchResult {
			// 未领取奖励的有三种可能 一种是群没有解散 一种是群解散了未领取 一种是过期了但是还没有打过期标签
			let who = ensure_signed(origin)?;
			let mut amount = <BalanceOf<T>>::from(0u32);
			// 一定要有加入的房间
			ensure!(<AllListeners<T>>::contains_key(who.clone()) && !<AllListeners<T>>::get(who.clone()).rooms.is_empty(), Error::<T>::NotIntoAnyRoom);
			let rooms = <AllListeners<T>>::get(who.clone()).rooms;

			let mut new_rooms = rooms.clone();

			for room in rooms.iter(){
				// 还没有领取才会去操作
				if room.1 == RewardStatus::NotGet{

					// 已经进入待奖励队列
					if !<AllRoom<T>>::contains_key(room.0.clone()){
						// 获取当前的session_index
						let session_index = Self::get_session_index();

						let mut is_get = false;

						// 超过20个session的算是过期

						for i in 0..20{
							let cur_session = session_index - (i as u32);
							if <InfoOfDisbandRoom<T>>::contains_key(cur_session, room.0.clone()){
								// 奖励本人
								let mut info = <InfoOfDisbandRoom<T>>::get(cur_session, room.0.clone());

								info.already_get_count += 1;

								let reward = info.per_man_reward;
								amount += reward.clone();
								info.already_get_reward += reward;

								<InfoOfDisbandRoom<T>>::insert(cur_session, room.0.clone(), info.clone());

								T::Create::on_unbalanced(T::Currency::deposit_creating(&who, reward));

								// 删除数据
								<ListenersOfRoom<T>>::remove(room.0.clone(), who.clone());

								if info.already_get_count.clone() == info.total_person.clone(){
									<ListenersOfRoom<T>>::remove(room.0.clone(), who.clone());
								}

								is_get = true;
								break;
							}

							// 一般不存在下面的问题
							if cur_session == 0{
								break;
							}
						}
						let mut status = RewardStatus::Expire;
						// 如果已经获取奖励(如果没有获取奖励 那么说明已经过期)
						if is_get {
							status = RewardStatus::Get;
						}

						// 修改状态
						new_rooms.retain(|h| h.0 != room.0.clone());
						new_rooms.push((room.0.clone(), status));

					}

				}
			}

			<AllListeners<T>>::mutate(who.clone(), |h| h.rooms = new_rooms);
			Self::deposit_event(RawEvent::Payout(who.clone(), amount));

			Ok(())
		}


		/// 在群里发红包
		#[weight = 10_000]
		pub fn send_redpacket_in_room(origin, group_id: u64, lucky_man_number: u32, amount: BalanceOf<T>) -> DispatchResult{
			let who = ensure_signed(origin)?;
			// 判断群是否已经创建 不存在则退出
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);
			// 自己要在群里
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);

			// 金额太小不能发红包
			ensure!(amount >= <BalanceOf<T>>::from(lucky_man_number).checked_mul(&T::RedPacketMinAmount::get()).ok_or(Error::<T>::Overflow)?, Error::<T>::AmountTooLow);

			T::ProposalRejection::on_unbalanced(T::Currency::withdraw(&who, amount.clone(), WithdrawReason::Transfer.into(), KeepAlive)?);

			// 获取红包id
			let redpacket_id = <RedPacketId>::get();

			let redpacket = RedPacket{
				id: redpacket_id,
				boss: who.clone(),
				total: amount.clone(),
				lucky_man_number: lucky_man_number,
				already_get_man: BTreeSet::<T::AccountId>::default(),
				min_amount_of_per_man: T::RedPacketMinAmount::get(),
				already_get_amount: <BalanceOf<T>>::from(0u32),
				end_time: Self::now() + T::RedPackExpire::get(),
			};

			<RedPacketOfRoom<T>>::insert(group_id, redpacket_id, redpacket);

			// 顺便处理过期红包
			Self::remove_redpacket_by_room_id(group_id, false);

			Self::deposit_event(RawEvent::SendRedPocket(group_id, redpacket_id, amount.clone()));

			Ok(())

		}


		/// 在群里收红包(需要基金会权限 比如基金会指定某个人可以领取多少)
		#[weight = 10_000]
		pub fn get_redpacket_in_room(origin, lucky_man: T::AccountId, group_id: u64, redpacket_id: u128, amount: BalanceOf<T>) {

			// 需要listen基金会权限
			let _ = T::ListenFounders::ensure_origin(origin)?;

			let who = lucky_man;

			// 判断群是否已经创建 不存在则退出
			ensure!(<AllRoom<T>>::contains_key(group_id), Error::<T>::RoomNotExists);
			// 自己要在群里
			ensure!(<ListenersOfRoom<T>>::contains_key(group_id, who.clone()), Error::<T>::NotInRoom);
			// 红包存在
			ensure!(<RedPacketOfRoom<T>>::contains_key(group_id, redpacket_id), Error::<T>::RedPacketNotExists);
			// 领取的金额足够大
//			ensure!(amount >= T::RedPacketMinAmount::get(), Error::<T>::AmountTooLow);


			let mut redpacket = <RedPacketOfRoom<T>>::get(group_id, redpacket_id);

			ensure!(amount >= redpacket.min_amount_of_per_man.clone(), Error::<T>::AmountTooLow);
			// 红包有足够余额
			ensure!(redpacket.total.clone() - redpacket.already_get_amount.clone() >= amount, Error::<T>::AmountNotEnough);
			// 红包领取人数不能超过最大
			ensure!(redpacket.lucky_man_number.clone() > (redpacket.already_get_man.clone().len() as u32), Error::<T>::ToMaxNumber);

			// 一个人只能领取一次
			ensure!(!redpacket.already_get_man.clone().contains(&who), Error::<T>::CountErr);

			// 过期删除数据 把剩余金额给本人
			if redpacket.end_time.clone() < Self::now(){
				let remain = redpacket.total.clone() - redpacket.already_get_amount.clone();
				T::Create::on_unbalanced(T::Currency::deposit_creating(&who, remain));
				<RedPacketOfRoom<T>>::remove(group_id, redpacket_id);

				return Err(Error::<T>::Expire)?;
			}

			T::Create::on_unbalanced(T::Currency::deposit_creating(&who, amount.clone()));

			redpacket.already_get_man.insert(who.clone());
			redpacket.already_get_amount += amount.clone();

			// 如果领取红包的人数已经达到上线 那么就把剩余的金额给本人 并删除记录
			if redpacket.already_get_man.clone().len() == (redpacket.lucky_man_number.clone() as usize){
				let remain = redpacket.total.clone() - redpacket.already_get_amount.clone();
				T::Create::on_unbalanced(T::Currency::deposit_creating(&who, remain));
				<RedPacketOfRoom<T>>::remove(group_id, redpacket_id);

			}

			if redpacket.already_get_amount.clone() == redpacket.total{
				<RedPacketOfRoom<T>>::remove(group_id, redpacket_id);
			}

			else{
				<RedPacketOfRoom<T>>::insert(group_id, redpacket_id, redpacket);
			}

			// 顺便处理过期红包
			Self::remove_redpacket_by_room_id(group_id, false);

			Self::deposit_event(RawEvent::GetRedPocket(group_id, redpacket_id, amount.clone()));

		}

	}
}


impl <T: Trait> Module <T> {


	// 加入群聊的操作
	fn join_do(you: T::AccountId, group_id: u64, inviter: Option<T::AccountId>, payment_type: Option<InvitePaymentType>) -> DispatchResult{

		let room_info = <AllRoom<T>>::get(group_id).unwrap();

		// 获取进群费用
		let join_cost = room_info.join_cost.clone();

		if inviter.is_some() {

			let inviter = inviter.unwrap();
			let payment_type = payment_type.unwrap();

			// 如果需要付费
			if join_cost > <BalanceOf<T>>::from(0u32){
				// 如果是邀请者自己出钱
				if payment_type == InvitePaymentType::inviter{
					// 扣除邀请者的钱(惩罚保留的)
					T::ProposalRejection::on_unbalanced(T::Currency::withdraw(&inviter, join_cost.clone(), WithdrawReason::Transfer.into(), KeepAlive)?);

					// 以铸币方式给其他账户转账
					Self::pay_for(group_id, join_cost);
				}
				// 如果是进群的人自己交费用
				else{
					T::ProposalRejection::on_unbalanced(T::Currency::withdraw(&you, join_cost.clone(), WithdrawReason::Transfer.into(), KeepAlive)?);
					Self::pay_for(group_id, join_cost);

				}

			}

			Self::remove_and_add_info(you.clone(), group_id, true)

		}

		// 如果自己不是被邀请进来的
		else {
			// 如果需要支付群费用
			if join_cost > <BalanceOf<T>>::from(0u32){
				T::ProposalRejection::on_unbalanced(T::Currency::withdraw(&you, join_cost.clone(), WithdrawReason::Transfer.into(), KeepAlive)?);
				Self::pay_for(group_id, join_cost);

			}
			Self::remove_and_add_info(you.clone(), group_id, false)

			}

		Ok(())
	}


	// 支付给其他人
	fn pay_for(group_id: u64, join_cost: BalanceOf<T>){
		let payment_manager_now = Percent::from_percent(5) * join_cost;
		let payment_manager_later = Percent::from_percent(5) * join_cost;
		let payment_room_later = Percent::from_percent(50) * join_cost;
		let payment_treasury = Percent::from_percent(40) * join_cost;
		let mut room_info = <AllRoom<T>>::get(group_id).unwrap();

		// 这些数据u128远远足够 不用特殊处理 要是panic  可以回家卖红薯
		room_info.total_balances += payment_room_later;
		room_info.total_balances += payment_manager_later;

		room_info.group_manager_balances += payment_manager_later;

		room_info.now_members_number += 1u32;
		let group_manager = room_info.group_manager.clone();
		<AllRoom<T>>::insert(group_id, room_info);

		// 给群主
		T::Create::on_unbalanced(T::Currency::deposit_creating(&group_manager, payment_manager_now));

		// 马上给国库
		let teasury_id = <treasury::Module<T>>::account_id();
		T::Create::on_unbalanced(T::Currency::deposit_creating(&teasury_id, payment_treasury));

	}


	// 进群的最后一步 添加与删除数据
	fn remove_and_add_info(yourself: T::AccountId, group_id: u64, is_invited: bool){

		let mut listener_type = ListenerType::default();

		// 添加信息
		<ListenersOfRoom<T>>::insert(group_id, yourself.clone(), listener_type);

		<AllListeners<T>>::mutate(yourself.clone(), |h| h.rooms.push((group_id, RewardStatus::default())));

		}


	// 获取现在的区块时间
	fn now() -> T::BlockNumber{
		<system::Module<T>>::block_number()
	}



	fn get_session_index() -> SessionIndex{
		0 as SessionIndex
	}


	// 根据房间号 对过期的红包进行处理
	fn remove_redpacket_by_room_id(room_id: u64, all: bool){
		let redpackets = <RedPacketOfRoom<T>>::iter_prefix(room_id).collect::<Vec<_>>();
		let now = Self::now();

		// 处理所有
		if all{
			for redpacket in redpackets.iter(){
				let who = redpacket.1.boss.clone();
				let remain = redpacket.1.total.clone() - redpacket.1.already_get_amount.clone();
				let redpacket_id = redpacket.0.clone();
				T::Create::on_unbalanced(T::Currency::deposit_creating(&who, remain));

				<RedPacketOfRoom<T>>::remove(room_id, redpacket_id);
		}

		}

			// 处理过期的红包
		else{
			for redpacket in redpackets.iter(){
				// 如果过期
				if redpacket.1.end_time < now{
					let who = redpacket.1.boss.clone();
					let remain = redpacket.1.total.clone() - redpacket.1.already_get_amount.clone();
					let redpacket_id = redpacket.0.clone();
					T::Create::on_unbalanced(T::Currency::deposit_creating(&who, remain));

					<RedPacketOfRoom<T>>::remove(room_id, redpacket_id);
				}
		}

		}


	}

	// 判断投票是否结束 (结束 , 通过)
	fn is_vote_end(total_count: u32, vote_info: DisbandVote<BTreeSet<T::AccountId>>, start_time: T::BlockNumber) -> (bool, bool){
		let half = ((total_count + 1) / 2) as usize;
		let total_count = total_count as usize;
		// 如果有票数超过一半
		if vote_info.approve_man.clone().len() >= half || vote_info.reject_man.clone().len() >= half{
			if vote_info.approve_man.clone().len() >= half {
				 (End, Pass)

			}
			else{
				 (End, NotPass)
			}
		}

		else{
			let max = cmp::max(vote_info.approve_man.clone().len(), vote_info.reject_man.clone().len());
			let min = cmp::min(vote_info.approve_man.clone().len(), vote_info.reject_man.clone().len());
			if Percent::from_percent(20) * total_count < max - min{
				if vote_info.approve_man.clone().len() >= vote_info.reject_man.clone().len(){
					 (End, Pass)
				}
				else{
					 (End, NotPass)
				}

			}

			else{
				if start_time + T::VoteExistsHowLong::get() >= Self::now(){
					(NotEnd, NotPass)
				}

				// 时间到 结束
				else{
					(End, NotPass)
				}

			}
		}

		// 第一个是是否结束 第二个是是否通过
	}


	// 删除过期的解散群产生的信息
	fn remove_expire_disband_info() {
		let session_indexs = <AllSessionIndex>::get();

		let mut session_indexs_cp = session_indexs.clone();

		// 注意  这个执行一次就出来了
		for index in session_indexs.iter(){
			let cur_session_index = Self::get_session_index();
			if cur_session_index - index >= 84u32{
				let mut info = <InfoOfDisbandRoom<T>>::iter_prefix(index).collect::<Vec<_>>();
				// 一次哦删除最多一个session 200条数据
				info.truncate(200);
				for i in info.iter(){
					let group_id = i.0;
					// 删除掉房间剩余记录
					<ListenersOfRoom<T>>::remove_prefix(group_id);

					let disband_room_info = <InfoOfDisbandRoom<T>>::get(index, group_id);
					// 获取剩余的没有领取的金额
					let remain_reward = disband_room_info.total_reward - disband_room_info.already_get_reward;

					// 剩余的金额转给国库
					let teasury_id = <treasury::Module<T>>::account_id();
					T::Create::on_unbalanced(T::Currency::deposit_creating(&teasury_id, remain_reward));

					// 每个房间
					<InfoOfDisbandRoom<T>>::remove(index, group_id);

				}

				// 如果已经完全删除 那么把这个index去掉
				let info1 = <InfoOfDisbandRoom<T>>::iter_prefix(index).collect::<Vec<_>>();
				if info1.is_empty(){
					<AllSessionIndex>::put(session_indexs_cp.split_off(1));
				}

			}
			break;

		}

	}




	}



decl_event!(
	pub enum Event<T> where
	 <T as system::Trait>::AccountId,
	 Amount = <<T as treasury::Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance,
// 	 CallHash = [u8; 32],
	 {
	 SetMultisig,
	 AirDroped(AccountId, AccountId),
	 CreatedRoom(AccountId, u64),
	 Invited(AccountId, AccountId),
	 IntoRoom(AccountId, u64),
	 RejectedInvite(AccountId, u64),
	 ChangedPermission(AccountId, u64),
	 BuyProps(AccountId),
	 BuyAudio(AccountId),
	 Kicked(AccountId, u64),
	 AskForDisband(AccountId, u64),
	 DisbandVote(AccountId, u64),
	 Payout(AccountId, Amount),
	 SendRedPocket(u64, u128, Amount),
	 GetRedPocket(u64, u128, Amount),
	 JoinCostChanged(u64, Amount),

	}
);





